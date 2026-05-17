package com.stellasecret.cvgenerator.ui

import com.stellasecret.cvgenerator.BuildConfig
import android.net.Uri
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.stellasecret.cvgenerator.data.model.*
import com.stellasecret.cvgenerator.data.repository.AiRepository
import com.stellasecret.cvgenerator.data.repository.AuthRepository
import com.stellasecret.cvgenerator.data.repository.DocumentRepository
import com.stellasecret.cvgenerator.data.repository.PreferencesRepository
import com.stellasecret.cvgenerator.data.repository.RemoteConfigRepository
import com.google.android.gms.auth.api.signin.GoogleSignInAccount
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.flow.*
import kotlinx.coroutines.tasks.await
import kotlinx.coroutines.launch
import javax.inject.Inject

@HiltViewModel
class MainViewModel @Inject constructor(
    private val authRepository: AuthRepository,
    private val aiRepository: AiRepository,
    private val remoteConfigRepository: RemoteConfigRepository,
    private val documentRepository: DocumentRepository,
    private val preferencesRepository: PreferencesRepository
) : ViewModel() {

    // ── Auth ──────────────────────────────────────────────────────────────────
    private val _authState = MutableStateFlow<AuthState>(AuthState.Loading)
    val authState: StateFlow<AuthState> = _authState.asStateFlow()

    // ── AI Config ─────────────────────────────────────────────────────────────
    val selectedModel: StateFlow<AiModel> = preferencesRepository.selectedModelFlow
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5000), AiModels.DEFAULT)

    // Cache des clés par provider (on écoute seulement le provider courant)
    private val _apiKeyCache = MutableStateFlow<Map<AiProvider, String>>(emptyMap())

    fun apiKeyForProvider(provider: AiProvider): StateFlow<String?> =
        preferencesRepository.apiKeyFlow(provider)
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5000), null)

    // ── Profile ───────────────────────────────────────────────────────────────
    private val _linkedInProfile = MutableStateFlow<LinkedInProfile?>(null)
    val linkedInProfile: StateFlow<LinkedInProfile?> = _linkedInProfile.asStateFlow()

    private val _linkedInLoading = MutableStateFlow(false)
    val linkedInLoading: StateFlow<Boolean> = _linkedInLoading.asStateFlow()

    private val _linkedInError = MutableStateFlow<String?>(null)
    val linkedInError: StateFlow<String?> = _linkedInError.asStateFlow()

    // ── Job Description ───────────────────────────────────────────────────────
    private val _jobDescription = MutableStateFlow<JobDescription>(JobDescription.None)
    val jobDescription: StateFlow<JobDescription> = _jobDescription.asStateFlow()

    private val _jobDescLoading = MutableStateFlow(false)
    val jobDescLoading: StateFlow<Boolean> = _jobDescLoading.asStateFlow()

    private val _jobDescError = MutableStateFlow<String?>(null)
    val jobDescError: StateFlow<String?> = _jobDescError.asStateFlow()

    // ── Generation ────────────────────────────────────────────────────────────
    private val _generationState = MutableStateFlow<GenerationState>(GenerationState.Idle)
    val generationState: StateFlow<GenerationState> = _generationState.asStateFlow()

    // ── Snackbar ──────────────────────────────────────────────────────────────
    private val _snackbarMessage = MutableSharedFlow<String>()
    val snackbarMessage: SharedFlow<String> = _snackbarMessage.asSharedFlow()

    init { checkCurrentUser() }

    // ── Auth ──────────────────────────────────────────────────────────────────

    private fun checkCurrentUser() {
        val user = authRepository.getCurrentUser()
        _authState.value = if (user != null) AuthState.Authenticated(user) else AuthState.Unauthenticated
    }

    fun signInWithGoogle(account: GoogleSignInAccount) {
        viewModelScope.launch {
            _authState.value = AuthState.Loading
            authRepository.signInWithGoogle(account)
                .onSuccess { user -> _authState.value = AuthState.Authenticated(user) }
                .onFailure { e ->
                    _authState.value = AuthState.Unauthenticated
                    _snackbarMessage.emit("Connexion échouée : ${e.message}")
                }
        }
    }

    fun signOut() {
        authRepository.signOut()
        _authState.value = AuthState.Unauthenticated
    }

    // ── AI Config ─────────────────────────────────────────────────────────────

    fun selectModel(model: AiModel) {
        viewModelScope.launch { preferencesRepository.saveSelectedModel(model) }
    }

    fun saveApiKey(provider: AiProvider, key: String) {
        viewModelScope.launch {
            preferencesRepository.saveApiKey(provider, key)
            _snackbarMessage.emit("Clé ${provider.displayName} enregistrée")
        }
    }

    // ── Profile ───────────────────────────────────────────────────────────────

    fun loadProfileFile(uri: Uri, fileName: String) {
        viewModelScope.launch {
            _linkedInLoading.value = true
            _linkedInError.value = null
            documentRepository.extractText(uri)
                .onSuccess { text ->
                    _linkedInProfile.value = LinkedInProfile(rawText = text, fileName = fileName, uri = uri)
                }
                .onFailure { e -> _linkedInError.value = e.message }
            _linkedInLoading.value = false
        }
    }

    fun loadProfileText(text: String) {
        _linkedInError.value = null
        _linkedInProfile.value = LinkedInProfile(rawText = text, fileName = "Texte saisi", uri = null)
    }

    fun clearLinkedInProfile() {
        _linkedInProfile.value = null
        _linkedInError.value = null
    }

    // ── Job Description ───────────────────────────────────────────────────────

    fun loadJobDescriptionFile(uri: Uri, fileName: String) {
        viewModelScope.launch {
            _jobDescLoading.value = true
            _jobDescError.value = null
            documentRepository.extractText(uri)
                .onSuccess { text -> _jobDescription.value = JobDescription.FromFile(uri, text, fileName) }
                .onFailure { e -> _jobDescError.value = e.message }
            _jobDescLoading.value = false
        }
    }

    fun setJobDescriptionText(text: String) {
        _jobDescription.value = if (text.isBlank()) JobDescription.None else JobDescription.FromText(text)
    }

    fun clearJobDescription() {
        _jobDescription.value = JobDescription.None
    }

    // ── CV Generation ─────────────────────────────────────────────────────────

    fun generateCV() {
        val profile = _linkedInProfile.value ?: run {
            viewModelScope.launch { _snackbarMessage.emit("Veuillez d'abord importer ou saisir votre profil") }
            return
        }

        val model = selectedModel.value

        viewModelScope.launch {
            // Retrieve the credential for the selected provider
            val isPremiumUser = (authState.value as? AuthState.Authenticated)?.user?.isPremium == true

            val apiKey: String? = when (model.provider) {
                AiProvider.VERTEX_AI -> {
                    if (isPremiumUser) {
                        // For premium users, we use the shared Gemini API key (AI Studio)
                        // instead of direct Vertex AI OAuth to avoid 403/permission issues.
                        remoteConfigRepository.getGeminiApiKey()
                    } else {
                        authRepository.getVertexAiAccessToken()
                    }
                }
                AiProvider.GEMINI -> {
                    if (isPremiumUser) {
                        // Premium: decrypt Gemini key from Firebase Remote Config
                        // Key is AES-256-GCM encrypted — plaintext never in APK or source
                        remoteConfigRepository.getGeminiApiKey()?.trim()
                    } else {
                        // Non-premium: use user's own key from DataStore
                        preferencesRepository.apiKeyFlow(model.provider).firstOrNull()?.trim()
                    }
                }
                AiProvider.ANTHROPIC,
                AiProvider.OPENAI -> {
                    preferencesRepository.apiKeyFlow(model.provider).firstOrNull()?.trim()
                }
            }

            val isPremiumShared = (model.provider == AiProvider.GEMINI || model.provider == AiProvider.VERTEX_AI) && isPremiumUser

            if (apiKey.isNullOrBlank()) {
                val errorMessage = when (model.provider) {
                    AiProvider.VERTEX_AI -> if (isPremiumUser) {
                        val encryptionKey = BuildConfig.GEMINI_ENCRYPTION_KEY
                        when {
                            encryptionKey.isBlank() -> "Erreur interne : clé de chiffrement manquante dans le build."
                            else -> "Clé Premium indisponible (Remote Config). Vérifiez votre connexion."
                        }
                    } else {
                        "Token Vertex AI indisponible. Déconnectez-vous puis reconnectez-vous."
                    }
                    AiProvider.ANTHROPIC,
                    AiProvider.OPENAI,
                    AiProvider.GEMINI -> if (isPremiumShared) {
                        val encryptionKey = BuildConfig.GEMINI_ENCRYPTION_KEY
                        when {
                            encryptionKey.isBlank() -> "Erreur interne : clé de chiffrement manquante dans le build."
                            else -> "Clé Gemini Premium indisponible. Vérifiez votre connexion."
                        }
                    } else {
                        "Clé API Gemini manquante. Configurez-la dans les paramètres."
                    }
                }
                _snackbarMessage.emit(errorMessage)
                return@launch
            }

            _generationState.value = GenerationState.Loading

            val jobText = when (val jd = _jobDescription.value) {
                is JobDescription.FromFile -> jd.rawText
                is JobDescription.FromText -> jd.text
                is JobDescription.None     -> null
            }

            // All providers use direct API call.
            // For premium Gemini, apiKey was already resolved from Remote Config above.
            val result = aiRepository.generateCV(
                apiKey             = apiKey,
                model              = model,
                profileText        = profile.rawText,
                jobDescriptionText = jobText
            )

            result.onSuccess { html ->
                _generationState.value = GenerationState.Success(
                    GeneratedCV(content = html, htmlContent = html, jobTitle = extractJobTitle(jobText))
                )
            }.onFailure { e ->
                _generationState.value = GenerationState.Error(e.message ?: "Erreur inconnue")
            }
        }
    }

    fun resetGeneration() {
        _generationState.value = GenerationState.Idle
    }

    private fun extractJobTitle(jobText: String?): String? =
        jobText?.lines()?.take(5)?.firstOrNull { it.length in 5..80 }?.trim()

    fun getGoogleSignInClient() = authRepository.googleSignInClient
}
