package com.stellasecret.cvgenerator.data.repository

import com.stellasecret.cvgenerator.data.model.AiModel
import com.stellasecret.cvgenerator.data.model.AiProvider
import com.stellasecret.cvgenerator.data.model.AnthropicContent
import com.stellasecret.cvgenerator.data.model.AnthropicMessage
import com.stellasecret.cvgenerator.data.model.AnthropicRequest
import com.stellasecret.cvgenerator.data.model.AnthropicResponse
import com.google.gson.Gson
import com.google.gson.JsonObject
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.logging.HttpLoggingInterceptor
import java.util.concurrent.TimeUnit
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class AiRepository @Inject constructor() {

    private val gson = Gson()

    private val client: OkHttpClient by lazy {
        OkHttpClient.Builder()
            .addInterceptor(HttpLoggingInterceptor().apply {
                level = HttpLoggingInterceptor.Level.BASIC
            })
            .connectTimeout(60, TimeUnit.SECONDS)
            .readTimeout(120, TimeUnit.SECONDS)
            .writeTimeout(60, TimeUnit.SECONDS)
            .build()
    }

    // ── Public entry point ────────────────────────────────────────────────────

    /**
     * [apiKey] semantics per provider:
     *   ANTHROPIC  → Anthropic API key (starts with sk-ant)
     *   OPENAI     → OpenAI API key (sk-…)
     *   GEMINI     → Google AI Studio API key (AIza…)
     *   VERTEX_AI  → Google OAuth2 access token (obtained from FirebaseAuth / GoogleSignIn)
     */
    suspend fun generateCV(
        apiKey: String,
        model: AiModel,
        profileText: String,
        jobDescriptionText: String?,
        vertexProjectId: String = "cvgenerator-project",
        vertexLocation: String  = "us-central1"
    ): Result<String> = when {
        model.provider == AiProvider.ANTHROPIC -> callAnthropic(apiKey, model, profileText, jobDescriptionText)
        model.provider == AiProvider.OPENAI    -> callOpenAI(apiKey, model, profileText, jobDescriptionText)

        // If provider is GEMINI OR if it's GEMINI_PREMIUM but we have an API key (starts with AIza)
        model.provider == AiProvider.GEMINI ||
        (model.provider == AiProvider.GEMINI_PREMIUM && apiKey.startsWith("AIza")) ->
            callGemini(apiKey, model, profileText, jobDescriptionText)

        model.provider == AiProvider.GEMINI_PREMIUM -> callGeminiPremium(
            accessToken   = apiKey,
            model         = model,
            profileText   = profileText,
            jobDescText   = jobDescriptionText,
            projectId     = vertexProjectId,
            location      = vertexLocation
        )

        else -> Result.failure(Exception("Fournisseur inconnu"))
    }

    // ── Prompts communs ───────────────────────────────────────────────────────

    private fun systemPrompt() = """
        Tu es un expert en recrutement et rédaction de CV de renommée mondiale.
        Ton objectif est de transformer un profil brut en un CV HTML d'élite, exhaustif et parfaitement formaté.

        Règles de rédaction (CRUCIALES) :
        - Inclus l'INTÉGRALITÉ des expériences professionnelles pertinentes du profil. Ne résume pas à l'extrême.
        - Pour chaque poste : Titre, Entreprise, Dates, et une liste détaillée des réalisations et responsabilités.
        - Utilise des verbes d'action puissants et chiffre les résultats (ex: "Augmentation de 30% du CA").
        - Structure : En-tête (Contact) → Résumé Professionnel → Expériences (la section la plus longue) → Compétences Techniques/Soft Skills → Formation → Langues.

        Règles Techniques (HTML/CSS) :
        - Génère un document HTML5 complet avec styles CSS inline dans une balise <style>.
        - Le design doit être moderne, épuré, utilisant des polices sans-serif (Arial, Helvetica).
        - Utilise une mise en page structurée (ex: colonne latérale pour les infos de contact/compétences et colonne principale pour les expériences).
        - Assure-toi que le contenu ne soit PAS tronqué. Si le profil est long, le CV peut faire plusieurs pages.
        - Réponds UNIQUEMENT avec le code HTML, sans texte de présentation avant ou après.
    """.trimIndent()

    private fun userMessage(profileText: String, jobDescriptionText: String?) =
        if (jobDescriptionText != null) """
            Voici le PROFIL complet de l'utilisateur :
            $profileText

            Voici la FICHE DE POSTE cible :
            $jobDescriptionText

            CONSIGNE :
            Génère un CV HTML complet et ultra-professionnel. Adapte le contenu pour qu'il résonne avec la fiche de poste (mots-clés, compétences mises en avant), mais conserve TOUTE la chronologie des expériences.
        """.trimIndent()
        else """
            Voici le PROFIL complet de l'utilisateur :
            $profileText

            CONSIGNE :
            Génère un CV HTML exhaustif et élégant à partir de ce profil. Inclus toutes les expériences, formations et compétences sans exception.
        """.trimIndent()

    // ── Anthropic ─────────────────────────────────────────────────────────────

    private suspend fun callAnthropic(
        apiKey: String,
        model: AiModel,
        profileText: String,
        jobDescriptionText: String?
    ): Result<String> = withContext(Dispatchers.IO) {
        try {
            val body = gson.toJson(
                AnthropicRequest(
                    model    = model.id,
                    system   = systemPrompt(),
                    messages = listOf(AnthropicMessage("user", userMessage(profileText, jobDescriptionText)))
                )
            ).toRequestBody("application/json".toMediaType())

            val request = Request.Builder()
                .url(model.provider.apiBaseUrl)
                .addHeader("x-api-key", apiKey)
                .addHeader("anthropic-version", "2023-06-01")
                .addHeader("content-type", "application/json")
                .post(body)
                .build()

            val response = client.newCall(request).execute()
            if (!response.isSuccessful) {
                return@withContext Result.failure(
                    Exception("Anthropic ${response.code}: ${response.body?.string()}")
                )
            }
            val parsed = gson.fromJson(response.body!!.string(), AnthropicResponse::class.java)
            val text = parsed.content.filter { it.type == "text" }.mapNotNull { it.text }.joinToString("")
            Result.success(cleanHtmlResponse(text))
        } catch (e: Exception) {
            Result.failure(e)
        }
    }

    // ── OpenAI ────────────────────────────────────────────────────────────────

    private suspend fun callOpenAI(
        apiKey: String,
        model: AiModel,
        profileText: String,
        jobDescriptionText: String?
    ): Result<String> = withContext(Dispatchers.IO) {
        try {
            val payload = JsonObject().apply {
                addProperty("model", model.id)
                addProperty("max_tokens", 8192)
                add("messages", gson.toJsonTree(listOf(
                    mapOf("role" to "system", "content" to systemPrompt()),
                    mapOf("role" to "user",   "content" to userMessage(profileText, jobDescriptionText))
                )))
            }

            val request = Request.Builder()
                .url(model.provider.apiBaseUrl)
                .addHeader("Authorization", "Bearer $apiKey")
                .addHeader("Content-Type", "application/json")
                .post(gson.toJson(payload).toRequestBody("application/json".toMediaType()))
                .build()

            val response = client.newCall(request).execute()
            if (!response.isSuccessful) {
                return@withContext Result.failure(
                    Exception("OpenAI ${response.code}: ${response.body?.string()}")
                )
            }
            val json   = gson.fromJson(response.body!!.string(), JsonObject::class.java)
            val text   = json["choices"].asJsonArray[0].asJsonObject["message"]
                .asJsonObject["content"].asString
            Result.success(cleanHtmlResponse(text))
        } catch (e: Exception) {
            Result.failure(e)
        }
    }


    // ── Gemini Premium ────────────────────────────────────────────────────────
    // Auth: Google OAuth2 access token passed as Bearer header.
    // The token is retrieved from GoogleSignIn (via getAccessToken) in the ViewModel.
    // Gemini Premium uses the same Gemini model IDs but goes through Google Cloud endpoints.
    private suspend fun callGeminiPremium(
        accessToken: String,
        model: AiModel,
        profileText: String,
        jobDescText: String?,
        projectId: String,
        location: String
    ): Result<String> = withContext(Dispatchers.IO) {
        try {
            val url = "https://$location-aiplatform.googleapis.com/v1/projects/$projectId" +
                      "/locations/$location/publishers/google/models/${model.id}:generateContent"

            val payload = JsonObject().apply {
                add("system_instruction", gson.toJsonTree(
                    mapOf("parts" to listOf(mapOf("text" to systemPrompt())))
                ))
                add("contents", gson.toJsonTree(listOf(
                    mapOf("role" to "user", "parts" to listOf(
                        mapOf("text" to userMessage(profileText, jobDescText))
                    ))
                )))
                add("generationConfig", gson.toJsonTree(
                    mapOf("maxOutputTokens" to 8192)
                ))
            }

            val request = Request.Builder()
                .url(url)
                .addHeader("Authorization", "Bearer $accessToken")
                .addHeader("Content-Type", "application/json")
                .post(gson.toJson(payload).toRequestBody("application/json".toMediaType()))
                .build()

            val response = client.newCall(request).execute()
            if (!response.isSuccessful) {
                val errorBody = response.body?.string() ?: ""
                android.util.Log.e("GeminiPremium", "Error ${response.code}: $errorBody")

                val msg = when (response.code) {
                    429 -> "Limite Premium atteinte. Réessayez plus tard."
                    403 -> "Accès Premium refusé. Vérifiez votre compte."
                    else -> "Erreur Premium ${response.code}"
                }
                return@withContext Result.failure(Exception(msg))
            }
            val json = gson.fromJson(response.body!!.string(), JsonObject::class.java)
            val parts = json["candidates"].asJsonArray[0].asJsonObject["content"]
                .asJsonObject["parts"].asJsonArray
            val text = parts.joinToString("") { it.asJsonObject["text"].asString }
            Result.success(cleanHtmlResponse(text))
        } catch (e: Exception) {
            Result.failure(e)
        }
    }

    // ── Gemini ────────────────────────────────────────────────────────────────

    private suspend fun callGemini(
        apiKey: String,
        model: AiModel,
        profileText: String,
        jobDescriptionText: String?
    ): Result<String> = withContext(Dispatchers.IO) {
        try {
            // Gemini v1beta : handles system_instruction field
            val payload = JsonObject().apply {
                add("system_instruction", gson.toJsonTree(
                    mapOf("parts" to listOf(mapOf("text" to systemPrompt())))
                ))
                add("contents", gson.toJsonTree(listOf(
                    mapOf("role" to "user", "parts" to listOf(
                        mapOf("text" to userMessage(profileText, jobDescriptionText))
                    ))
                )))
                add("generationConfig", gson.toJsonTree(
                    mapOf("maxOutputTokens" to 8192)
                ))
            }

            val baseUrl = AiProvider.GEMINI.apiBaseUrl
            val modelId = model.id
            val url = "$baseUrl/$modelId:generateContent?key=$apiKey"

            android.util.Log.d("GeminiAPI", "Calling URL: ${baseUrl}/$modelId:generateContent?key=${apiKey.take(4)}... (Len: ${apiKey.length})")

            val request = Request.Builder()
                .url(url)
                .post(gson.toJson(payload).toRequestBody("application/json".toMediaType()))
                .build()
            val response = client.newCall(request).execute()
            if (!response.isSuccessful) {
                val errorBody = response.body?.string() ?: ""
                android.util.Log.e("GeminiAPI", "Error ${response.code}: $errorBody")

                val msg = when (response.code) {
                    429 -> "Limite de requêtes atteinte. Réessayez dans une minute."
                    404 -> "Modèle IA non trouvé. Contactez le support."
                    else -> "Erreur Gemini ${response.code}"
                }
                return@withContext Result.failure(Exception(msg))
            }
            val json = gson.fromJson(response.body!!.string(), JsonObject::class.java)
            val parts = json["candidates"].asJsonArray[0].asJsonObject["content"]
                .asJsonObject["parts"].asJsonArray
            val text = parts.joinToString("") { it.asJsonObject["text"].asString }
            Result.success(cleanHtmlResponse(text))
        } catch (e: Exception) {
            Result.failure(e)
        }
    }

    /**
     * Extracts pure HTML content from the AI response.
     * Strips markdown code blocks (```html ... ```) and any leading/trailing text.
     */
    private fun cleanHtmlResponse(rawText: String): String {
        var text = rawText.trim()

        // 1. Strip markdown code blocks if present (case-insensitive)
        val lowerText = text.lowercase()
        if (lowerText.contains("```html")) {
            val startIdx = lowerText.indexOf("```html") + 7
            val endIdx = lowerText.lastIndexOf("```")
            text = if (endIdx > startIdx) {
                text.substring(startIdx, endIdx)
            } else {
                text.substring(startIdx)
            }
        } else if (text.contains("```")) {
            val startIdx = text.indexOf("```") + 3
            val endIdx = text.lastIndexOf("```")
            text = if (endIdx > startIdx) {
                text.substring(startIdx, endIdx)
            } else {
                text.substring(startIdx)
            }
        }

        text = text.trim()

        // 2. Further refine: ensure we start with <!DOCTYPE or <html
        val htmlStart = text.indexOf("<html", ignoreCase = true)
        val doctypeStart = text.indexOf("<!DOCTYPE", ignoreCase = true)

        val start = if (doctypeStart != -1 && (htmlStart == -1 || doctypeStart < htmlStart)) doctypeStart else htmlStart

        // If we found a clear HTML start, return from there.
        // We don't strictly require </html> at the end in case of truncation.
        return if (start != -1) {
            text.substring(start).trim()
        } else {
            text.trim()
        }
    }
}
