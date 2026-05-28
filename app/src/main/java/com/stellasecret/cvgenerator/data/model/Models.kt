package com.stellasecret.cvgenerator.data.model

import android.net.Uri

// ─── AI Providers & Models ────────────────────────────────────────────────────

enum class AiProvider(
    val displayName: String,
    val apiBaseUrl: String,
    val keyLabel: String,       // label affiché dans l'UI
    val keyHint: String,        // hint dans le champ texte
    val keyUrl: String          // lien pour obtenir la clé
) {
    ANTHROPIC(
        displayName  = "Anthropic (Claude)",
        apiBaseUrl   = "https://api.anthropic.com/v1/messages",
        keyLabel     = "Clé API Anthropic",
        keyHint      = "sk-ant‑…",
        keyUrl       = "https://console.anthropic.com"
    ),
    OPENAI(
        displayName  = "OpenAI (GPT)",
        apiBaseUrl   = "https://api.openai.com/v1/chat/completions",
        keyLabel     = "Clé API OpenAI",
        keyHint      = "sk-…",
        keyUrl       = "https://platform.openai.com/api-keys"
    ),
    GEMINI(
        displayName  = "Google Gemini",
        apiBaseUrl   = "https://generativelanguage.googleapis.com/v1beta/models",
        keyLabel     = "Clé API Google AI Studio",
        keyHint      = "AIza…",
        keyUrl       = "https://aistudio.google.com/app/apikey"
    ),
    GEMINI_PREMIUM(
        displayName  = "Gemini Premium",
        // URL built dynamically in AiRepository using project + location
        apiBaseUrl   = "https://us-central1-aiplatform.googleapis.com/v1",
        keyLabel     = "Accès premium activé",
        keyHint      = "Authentification automatique",
        keyUrl       = "https://console.cloud.google.com/vertex-ai"
    );

    val requiresPremium: Boolean get() = this == GEMINI_PREMIUM

    companion object {
        val DEFAULT = ANTHROPIC
        fun fromName(name: String): AiProvider =
            entries.firstOrNull { it.name == name } ?: DEFAULT
    }
}

data class AiModel(
    val id: String,
    val displayName: String,
    val description: String,
    val provider: AiProvider
)

object AiModels {
    // ── Anthropic ─────────────────────────────────────────────────────────────
    val CLAUDE_OPUS_4 = AiModel(
        id          = "claude-opus-4-20250514",
        displayName = "Claude Opus 4",
        description = "Meilleur résultat — plus lent, plus coûteux",
        provider    = AiProvider.ANTHROPIC
    )
    val CLAUDE_SONNET_4 = AiModel(
        id          = "claude-sonnet-4-20250514",
        displayName = "Claude Sonnet 4",
        description = "Équilibre qualité / vitesse — recommandé",
        provider    = AiProvider.ANTHROPIC
    )
    val CLAUDE_HAIKU_3_5 = AiModel(
        id          = "claude-haiku-3-5-20241022",
        displayName = "Claude Haiku 3.5",
        description = "Le plus rapide — idéal pour tester",
        provider    = AiProvider.ANTHROPIC
    )

    // ── OpenAI ────────────────────────────────────────────────────────────────
    val GPT_4O = AiModel(
        id          = "gpt-4o",
        displayName = "GPT-4o",
        description = "Modèle phare d'OpenAI — vision + texte",
        provider    = AiProvider.OPENAI
    )
    val GPT_4O_MINI = AiModel(
        id          = "gpt-4o-mini",
        displayName = "GPT-4o mini",
        description = "Rapide et économique",
        provider    = AiProvider.OPENAI
    )
    val GPT_4_1 = AiModel(
        id          = "gpt-4.1",
        displayName = "GPT-4.1",
        description = "Dernière génération OpenAI",
        provider    = AiProvider.OPENAI
    )

    // ── Gemini ────────────────────────────────────────────────────────────────
    val GEMINI_2_0_FLASH = AiModel(
        id          = "gemini-2.0-flash",
        displayName = "Gemini 2.0 Flash",
        description = "Rapide et multimodal",
        provider    = AiProvider.GEMINI
    )
    val GEMINI_2_5_FLASH = AiModel(
        id          = "gemini-2.5-flash",
        displayName = "Gemini 2.5 Flash",
        description = "Modèle stable et équilibré",
        provider    = AiProvider.GEMINI
    )

    // ── Gemini Premium (premium — auth via Google OAuth) ─────────────────────
    val VERTEX_GEMINI_2_0_FLASH = AiModel(
        id          = "gemini-2.0-flash",
        displayName = "Gemini 2.0 Flash",
        description = "⭑ Premium — Rapide et multimodal",
        provider    = AiProvider.GEMINI_PREMIUM
    )
    val VERTEX_GEMINI_2_5_PRO = AiModel(
        id          = "gemini-2.5-pro",
        displayName = "Gemini 2.5 Pro",
        description = "⭑ Premium — Stable et fiable",
        provider    = AiProvider.GEMINI_PREMIUM
    )

    // ── Index ─────────────────────────────────────────────────────────────────
    val all: List<AiModel> = listOf(
        CLAUDE_OPUS_4, CLAUDE_SONNET_4, CLAUDE_HAIKU_3_5,
        GPT_4O, GPT_4O_MINI, GPT_4_1,
        GEMINI_2_0_FLASH, GEMINI_2_5_FLASH,
        VERTEX_GEMINI_2_0_FLASH, VERTEX_GEMINI_2_5_PRO
    )

    val DEFAULT = CLAUDE_SONNET_4

    fun forProvider(provider: AiProvider): List<AiModel> =
        all.filter { it.provider == provider }

    fun fromId(id: String): AiModel =
        all.firstOrNull { it.id == id } ?: DEFAULT
}

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
    val fileName: String,          // nom affiché dans l'UI
    val uri: Uri? = null           // null si saisi via copier-coller
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
    val max_tokens: Int = 8192,
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
