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
    ): Result<String> = when (model.provider) {
        AiProvider.ANTHROPIC -> callAnthropic(apiKey, model, profileText, jobDescriptionText)
        AiProvider.OPENAI    -> callOpenAI(apiKey, model, profileText, jobDescriptionText)
        AiProvider.GEMINI    -> callGemini(apiKey, model, profileText, jobDescriptionText)
        AiProvider.VERTEX_AI -> callVertexAI(
            accessToken   = apiKey,
            model         = model,
            profileText   = profileText,
            jobDescText   = jobDescriptionText,
            projectId     = vertexProjectId,
            location      = vertexLocation
        )
    }

    // ── Prompts communs ───────────────────────────────────────────────────────

    private fun systemPrompt() = """
        Tu es un expert en recrutement et rédaction de CV avec 20 ans d'expérience.
        Tu crées des CV professionnels, percutants et adaptés aux offres d'emploi.

        Règles impératives :
        - Utilise un format HTML propre et bien structuré pour le CV
        - Mets en valeur les compétences clés correspondant à l'offre
        - Utilise des verbes d'action forts (développé, dirigé, optimisé, conçu...)
        - Quantifie les réalisations quand possible (% d'amélioration, nombre d'utilisateurs, etc.)
        - Structure : Résumé percutant → Expériences → Compétences → Formation → Langues
        - Adapte le vocabulaire aux mots-clés de l'offre pour passer les ATS
        - Longueur : 1-2 pages maximum
        - Réponds TOUJOURS avec du HTML valide et complet incluant les styles CSS inline
        - Le HTML doit être prêt à être converti en PDF
    """.trimIndent()

    private fun userMessage(profileText: String, jobDescriptionText: String?) =
        if (jobDescriptionText != null) """
            PROFIL :
            $profileText

            ---

            FICHE DE POSTE :
            $jobDescriptionText

            ---

            Génère un CV HTML complet et professionnel, parfaitement adapté à cette fiche de poste.
        """.trimIndent()
        else """
            PROFIL :
            $profileText

            ---

            Génère un CV HTML complet et professionnel à partir de ce profil.
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
            Result.success(text)
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
                addProperty("max_tokens", 4096)
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
            Result.success(text)
        } catch (e: Exception) {
            Result.failure(e)
        }
    }


    // ── Vertex AI ─────────────────────────────────────────────────────────────
    // Auth: Google OAuth2 access token passed as Bearer header.
    // The token is retrieved from GoogleSignIn (via getAccessToken) in the ViewModel.
    // Vertex AI uses the same Gemini model IDs but goes through Google Cloud endpoints.
    private suspend fun callVertexAI(
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
                    mapOf("maxOutputTokens" to 4096)
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
                return@withContext Result.failure(
                    Exception("Vertex AI ${response.code}: ${response.body?.string()}")
                )
            }
            val json = gson.fromJson(response.body!!.string(), JsonObject::class.java)
            val text = json["candidates"].asJsonArray[0].asJsonObject["content"]
                .asJsonObject["parts"].asJsonArray[0].asJsonObject["text"].asString
            Result.success(text)
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
            // Gemini : system instruction + user turn
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
                    mapOf("maxOutputTokens" to 4096)
                ))
            }

            val url = "${model.provider.apiBaseUrl}/${model.id}:generateContent?key=$apiKey"
            val request = Request.Builder()
                .url(url)
                .addHeader("Content-Type", "application/json")
                .post(gson.toJson(payload).toRequestBody("application/json".toMediaType()))
                .build()

            val response = client.newCall(request).execute()
            if (!response.isSuccessful) {
                return@withContext Result.failure(
                    Exception("Gemini ${response.code}: ${response.body?.string()}")
                )
            }
            val json = gson.fromJson(response.body!!.string(), JsonObject::class.java)
            val text = json["candidates"].asJsonArray[0].asJsonObject["content"]
                .asJsonObject["parts"].asJsonArray[0].asJsonObject["text"].asString
            Result.success(text)
        } catch (e: Exception) {
            Result.failure(e)
        }
    }
}
