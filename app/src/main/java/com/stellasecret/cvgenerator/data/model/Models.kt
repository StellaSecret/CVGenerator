package com.stellasecret.cvgenerator.data.model

import android.net.Uri

// ─── User & Auth ─────────────────────────────────────────────────────────────

data class User(
    val email: String,
    val displayName: String?,
    val photoUrl: String?,
    val isPremium: Boolean = false
)

sealed class AuthState {
    object Unauthenticated : AuthState()
    data class Authenticated(val user: User) : AuthState()
    object Loading : AuthState()
}

// ─── Document Inputs ──────────────────────────────────────────────────────────

data class LinkedInProfile(
    val rawText: String,
    val fileName: String,
    val uri: Uri? = null
)

sealed class JobDescription {
    data class FromFile(val uri: Uri, val rawText: String, val fileName: String) : JobDescription()
    data class FromText(val text: String) : JobDescription()
    object None : JobDescription()
}

// ─── AI Config ────────────────────────────────────────────────────────────────

sealed class AiConfig {
    data class VertexAi(val projectId: String = "cvgenerator-project") : AiConfig()
    data class AnthropicKey(val apiKey: String) : AiConfig()
}

// ─── CV Generation ────────────────────────────────────────────────────────────

data class GeneratedCV(
    val content: String,        // Markdown / structured text
    val htmlContent: String,    // Ready-to-render HTML
    val jobTitle: String?,
    val timestamp: Long = System.currentTimeMillis()
)

sealed class GenerationState {
    object Idle : GenerationState()
    object Loading : GenerationState()
    data class Success(val cv: GeneratedCV) : GenerationState()
    data class Error(val message: String) : GenerationState()
}

// ─── API Request/Response ─────────────────────────────────────────────────────

data class AnthropicRequest(
    val model: String = "claude-opus-4-20250514",
    val max_tokens: Int = 4096,
    val system: String,
    val messages: List<AnthropicMessage>
)

data class AnthropicMessage(
    val role: String,
    val content: String
)

data class AnthropicResponse(
    val content: List<AnthropicContent>,
    val usage: AnthropicUsage?
)

data class AnthropicContent(
    val type: String,
    val text: String?
)

data class AnthropicUsage(
    val input_tokens: Int,
    val output_tokens: Int
)
