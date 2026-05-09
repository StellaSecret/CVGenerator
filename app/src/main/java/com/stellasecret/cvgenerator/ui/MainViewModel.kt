package com.stellasecret.cvgenerator.ui

import android.net.Uri
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.stellasecret.cvgenerator.data.model.*
import com.stellasecret.cvgenerator.data.repository.AnthropicRepository
import com.stellasecret.cvgenerator.data.repository.AuthRepository
import com.stellasecret.cvgenerator.data.repository.DocumentRepository
import com.stellasecret.cvgenerator.data.repository.PreferencesRepository
import com.google.android.gms.auth.api.signin.GoogleSignInAccount
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.flow.*
import kotlinx.coroutines.launch
import javax.inject.Inject

@HiltViewModel
class MainViewModel @Inject constructor(
    private val authRepository: AuthRepository,
    private val anthropicRepository: AnthropicRepository,
    private val documentRepository: DocumentRepository,
    private val preferencesRepository: PreferencesRepository
) : ViewModel() {

    // ── Auth State ────────────────────────────────────────────────────────────
    private val _authState = MutableStateFlow<AuthState>(AuthState.Loading)
    val authState: StateFlow<AuthState> = _authState.asStateFlow()

    // ── API Key ───────────────────────────────────────────────────────────────
    val savedApiKey: StateFlow<String?> = preferencesRepository.apiKeyFlow
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5000), null)

    // ── LinkedIn Profile ──────────────────────────────────────────────────────
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

    init {
        checkCurrentUser()
    }

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

    fun saveApiKey(key: String) {
        viewModelScope.launch {
            preferencesRepository.saveApiKey(key)
            _snackbarMessage.emit("Clé API sauvegardée")
        }
    }

    // ── LinkedIn PDF ──────────────────────────────────────────────────────────

    fun loadLinkedInPdf(uri: Uri) {
        viewModelScope.launch {
            _linkedInLoading.value = true
            _linkedInError.value = null
            documentRepository.extractText(uri)
                .onSuccess { text ->
                    _linkedInProfile.value = LinkedInProfile(rawText = text, uri = uri)
                }
                .onFailure { e ->
                    _linkedInError.value = e.message
                }
            _linkedInLoading.value = false
        }
    }

    // ── Job Description ───────────────────────────────────────────────────────

    fun loadJobDescriptionFile(uri: Uri, fileName: String) {
        viewModelScope.launch {
            _jobDescLoading.value = true
            _jobDescError.value = null
            documentRepository.extractText(uri)
                .onSuccess { text ->
                    _jobDescription.value = JobDescription.FromFile(uri, text, fileName)
                }
                .onFailure { e ->
                    _jobDescError.value = e.message
                }
            _jobDescLoading.value = false
        }
    }

    fun setJobDescriptionText(text: String) {
        _jobDescription.value = if (text.isBlank()) JobDescription.None
        else JobDescription.FromText(text)
    }

    fun clearJobDescription() {
        _jobDescription.value = JobDescription.None
    }

    // ── CV Generation ─────────────────────────────────────────────────────────

    fun generateCV(apiKey: String?) {
        val profile = _linkedInProfile.value
        if (profile == null) {
            viewModelScope.launch { _snackbarMessage.emit("Veuillez d'abord charger votre profil LinkedIn") }
            return
        }

        val key = apiKey ?: savedApiKey.value
        if (key.isNullOrBlank()) {
            viewModelScope.launch { _snackbarMessage.emit("Clé API manquante. Veuillez la saisir dans les paramètres.") }
            return
        }

        viewModelScope.launch {
            _generationState.value = GenerationState.Loading

            val jobText = when (val jd = _jobDescription.value) {
                is JobDescription.FromFile -> jd.rawText
                is JobDescription.FromText -> jd.text
                is JobDescription.None -> null
            }

            anthropicRepository.generateCV(
                apiKey = key,
                linkedInText = profile.rawText,
                jobDescriptionText = jobText
            ).onSuccess { htmlContent ->
                _generationState.value = GenerationState.Success(
                    GeneratedCV(
                        content = htmlContent,
                        htmlContent = htmlContent,
                        jobTitle = extractJobTitle(jobText)
                    )
                )
            }.onFailure { e ->
                _generationState.value = GenerationState.Error(e.message ?: "Erreur inconnue")
            }
        }
    }

    fun resetGeneration() {
        _generationState.value = GenerationState.Idle
    }

    private fun extractJobTitle(jobText: String?): String? {
        if (jobText == null) return null
        val lines = jobText.lines().take(5)
        return lines.firstOrNull { it.length in 5..80 }?.trim()
    }

    fun getGoogleSignInClient() = authRepository.googleSignInClient
}
