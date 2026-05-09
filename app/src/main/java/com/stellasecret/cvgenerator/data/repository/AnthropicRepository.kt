package com.stellasecret.cvgenerator.data.repository

import com.stellasecret.cvgenerator.data.model.AnthropicMessage
import com.stellasecret.cvgenerator.data.model.AnthropicRequest
import com.stellasecret.cvgenerator.data.model.AnthropicResponse
import com.google.gson.Gson
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
class AnthropicRepository @Inject constructor() {

    private val gson = Gson()

    private fun buildClient(): OkHttpClient {
        val logging = HttpLoggingInterceptor().apply {
            level = HttpLoggingInterceptor.Level.BODY
        }
        return OkHttpClient.Builder()
            .addInterceptor(logging)
            .connectTimeout(60, TimeUnit.SECONDS)
            .readTimeout(120, TimeUnit.SECONDS)
            .writeTimeout(60, TimeUnit.SECONDS)
            .build()
    }

    suspend fun generateCV(
        apiKey: String,
        linkedInText: String,
        jobDescriptionText: String?
    ): Result<String> = withContext(Dispatchers.IO) {
        try {
            val systemPrompt = buildSystemPrompt()
            val userMessage = buildUserMessage(linkedInText, jobDescriptionText)

            val requestBody = AnthropicRequest(
                model = "claude-opus-4-20250514",
                max_tokens = 4096,
                system = systemPrompt,
                messages = listOf(AnthropicMessage("user", userMessage))
            )

            val json = gson.toJson(requestBody)
            val body = json.toRequestBody("application/json".toMediaType())

            val request = Request.Builder()
                .url("https://api.anthropic.com/v1/messages")
                .addHeader("x-api-key", apiKey)
                .addHeader("anthropic-version", "2023-06-01")
                .addHeader("content-type", "application/json")
                .post(body)
                .build()

            val client = buildClient()
            val response = client.newCall(request).execute()

            if (!response.isSuccessful) {
                val errorBody = response.body?.string() ?: "Unknown error"
                return@withContext Result.failure(Exception("API Error ${response.code}: $errorBody"))
            }

            val responseBody = response.body?.string()
                ?: return@withContext Result.failure(Exception("Empty response body"))

            val anthropicResponse = gson.fromJson(responseBody, AnthropicResponse::class.java)
            val cvContent = anthropicResponse.content
                .filter { it.type == "text" }
                .mapNotNull { it.text }
                .joinToString("")

            Result.success(cvContent)
        } catch (e: Exception) {
            Result.failure(e)
        }
    }

    private fun buildSystemPrompt(): String = """
        Tu es un expert en recrutement et rédaction de CV avec 20 ans d'expérience.
        Tu crées des CV professionnels, percutants et adaptés aux offres d'emploi.
        
        Règles impératives :
        - Utilise un format HTML propre et bien structuré pour le CV
        - Mets en valeur les compétences clés correspondant à l'offre
        - Utilise des verbes d'action forts (développé, dirigé, optimisé, conçu...)
        - Quantifie les réalisations quand possible (% d'amélioration, nombre d'utilisateurs, etc.)
        - Structure : Résumé percutant → Expériences → Compétences → Formation → Langues
        - Adapte le vocabulaire aux mots-clés de l'offre pour passer les ATS (systèmes de tri automatique)
        - Longueur : 1-2 pages maximum
        - Réponds TOUJOURS avec du HTML valide et complet incluant les styles CSS inline
        - Le HTML doit être prêt à être converti en PDF
    """.trimIndent()

    private fun buildUserMessage(linkedInText: String, jobDescriptionText: String?): String {
        return if (jobDescriptionText != null) {
            """
            PROFIL LINKEDIN :
            $linkedInText
            
            ---
            
            FICHE DE POSTE :
            $jobDescriptionText
            
            ---
            
            Génère un CV HTML complet et professionnel, parfaitement adapté à cette fiche de poste.
            Identifie les mots-clés importants de l'offre et intègre-les naturellement dans le CV.
            Mets en avant les expériences et compétences les plus pertinentes pour ce poste.
            """.trimIndent()
        } else {
            """
            PROFIL LINKEDIN :
            $linkedInText
            
            ---
            
            Génère un CV HTML complet, professionnel et percutant à partir de ce profil LinkedIn.
            Structure le CV de manière optimale et mets en valeur les compétences clés.
            """.trimIndent()
        }
    }
}
