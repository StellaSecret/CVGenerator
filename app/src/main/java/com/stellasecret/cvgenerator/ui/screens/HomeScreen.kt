package com.stellasecret.cvgenerator.ui.screens

import android.app.Activity
import android.content.Intent
import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.*
import androidx.compose.animation.core.tween
import androidx.compose.foundation.*
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.material3.HorizontalDivider
import androidx.compose.runtime.*
import androidx.compose.runtime.collectAsState
import androidx.compose.material3.FilterChip
import androidx.compose.foundation.clickable
import androidx.compose.ui.draw.clip
import androidx.compose.runtime.rememberCoroutineScope
import kotlinx.coroutines.launch
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import com.stellasecret.cvgenerator.data.model.*
import kotlinx.coroutines.flow.StateFlow
import com.stellasecret.cvgenerator.ui.MainViewModel
import com.stellasecret.cvgenerator.ui.components.*
import com.stellasecret.cvgenerator.ui.theme.*
import com.google.android.gms.auth.api.signin.GoogleSignIn
import com.google.android.gms.auth.api.signin.GoogleSignInStatusCodes
import com.google.android.gms.common.api.ApiException
import kotlinx.coroutines.flow.collectLatest

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HomeScreen(
    viewModel: MainViewModel = hiltViewModel(),
    onNavigateToResult: () -> Unit
) {
    val context = LocalContext.current
    val snackbarHostState = remember { SnackbarHostState() }
    val scope = rememberCoroutineScope()

    val authState by viewModel.authState.collectAsState()
    val linkedInProfile by viewModel.linkedInProfile.collectAsState()
    val linkedInLoading by viewModel.linkedInLoading.collectAsState()
    val linkedInError by viewModel.linkedInError.collectAsState()
    val jobDescription by viewModel.jobDescription.collectAsState()
    val jobDescLoading by viewModel.jobDescLoading.collectAsState()
    val jobDescError by viewModel.jobDescError.collectAsState()
    val generationState by viewModel.generationState.collectAsState()

    var showSettingsDialog by remember { mutableStateOf(false) }
    var showJobDescTextDialog by remember { mutableStateOf(false) }
    var jobDescTextInput by remember { mutableStateOf("") }

    // Collect snackbar messages
    LaunchedEffect(Unit) {
        viewModel.snackbarMessage.collectLatest {
            snackbarHostState.showSnackbar(it)
        }
    }

    // Navigate to result when CV is generated (one-time event)
    LaunchedEffect(Unit) {
        viewModel.navEvents.collectLatest { event ->
            if (event is MainViewModel.NavEvent.NavigateToResult) {
                onNavigateToResult()
            }
        }
    }

    // ── Google Sign-In launcher ────────────────────────────────────────────────
    val googleSignInLauncher = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.StartActivityForResult()
    ) { result ->
        if (result.resultCode == Activity.RESULT_OK) {
            try {
                val task = GoogleSignIn.getSignedInAccountFromIntent(result.data)
                val account = task.getResult(ApiException::class.java)
                viewModel.signInWithGoogle(account)
            } catch (e: ApiException) {
                val msg = when (e.statusCode) {
                    GoogleSignInStatusCodes.SIGN_IN_CANCELLED -> "Connexion annulée"
                    GoogleSignInStatusCodes.NETWORK_ERROR -> "Erreur réseau"
                    else -> "Erreur de connexion : ${e.statusCode}"
                }
                scope.launch { snackbarHostState.showSnackbar(msg) }
            }
        }
    }

    // ── Profil : picker fichier ───────────────────────────────────────────────
    val profileFilePicker = rememberLauncherForActivityResult(
        ActivityResultContracts.GetContent()
    ) { uri: Uri? ->
        uri?.let { picked ->
            val fileName = context.contentResolver.query(picked, null, null, null, null)?.use { cursor ->
                if (cursor.moveToFirst()) {
                    val idx = cursor.getColumnIndex(android.provider.OpenableColumns.DISPLAY_NAME)
                    if (idx >= 0) cursor.getString(idx) else "fichier"
                } else "fichier"
            } ?: "fichier"
            viewModel.loadProfileFile(picked, fileName)
        }
    }

    var showProfileTextDialog by remember { mutableStateOf(false) }

    // ── Job Description file picker ───────────────────────────────────────────
    val jobDescPicker = rememberLauncherForActivityResult(
        ActivityResultContracts.GetContent()
    ) { uri: Uri? ->
        uri?.let { picked ->
            val fileName = context.contentResolver.query(picked, null, null, null, null)?.use { cursor ->
                if (cursor.moveToFirst()) {
                    val idx = cursor.getColumnIndex(android.provider.OpenableColumns.DISPLAY_NAME)
                    if (idx >= 0) cursor.getString(idx) else "fichier"
                } else "fichier"
            } ?: "fichier"
            viewModel.loadJobDescriptionFile(picked, fileName)
        }
    }

    // ── Derive API key availability ───────────────────────────────────────────
    val selectedModel by viewModel.selectedModel.collectAsState()
    val canGenerate = linkedInProfile != null

    Scaffold(
        snackbarHost = { SnackbarHost(snackbarHostState) },
        containerColor = MaterialTheme.colorScheme.background
    ) { paddingValues ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(paddingValues)
                .verticalScroll(rememberScrollState())
        ) {
            // ── Header ────────────────────────────────────────────────────────
            GradientHeader()

            // ── Auth section ──────────────────────────────────────────────────
            Column(modifier = Modifier.padding(horizontal = 16.dp, vertical = 20.dp)) {

                AuthSection(
                    authState = authState,
                    onSignIn = {
                        val signInIntent = viewModel.getGoogleSignInClient().signInIntent
                        googleSignInLauncher.launch(signInIntent)
                    },
                    onSignOut = { viewModel.signOut() }
                )

                Spacer(Modifier.height(24.dp))

                // ── API Key (if not premium) ───────────────────────────────────
                ModelPickerSection(
                    selectedModel = selectedModel,
                    onConfigureClick = { showSettingsDialog = true }
                )
                Spacer(Modifier.height(24.dp))

                // ── Profil ───────────────────────────────────────────────────
                SectionHeader(title = "Votre profil", badge = "Requis")
                Spacer(Modifier.height(10.dp))

                when {
                    linkedInProfile != null -> {
                        UploadCard(
                            title = linkedInProfile!!.fileName,
                            subtitle = if (linkedInProfile!!.uri != null) "Fichier importé" else "Texte saisi",
                            icon = Icons.Outlined.Person,
                            isLoaded = true,
                            loadedFileName = linkedInProfile!!.fileName,
                            isLoading = linkedInLoading,
                            error = linkedInError,
                            onClick = {},
                            onClear = { viewModel.clearLinkedInProfile() }
                        )
                    }
                    linkedInLoading -> {
                        UploadCard(
                            title = "Chargement…",
                            subtitle = "",
                            icon = Icons.Outlined.Person,
                            isLoaded = false,
                            isLoading = true,
                            error = linkedInError,
                            onClick = {}
                        )
                    }
                    else -> {
                        // Afficher erreur si présente
                        if (linkedInError != null) {
                            Text(
                                linkedInError!!,
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.error,
                                modifier = Modifier.padding(bottom = 8.dp)
                            )
                        }
                        // Deux boutons : Fichier + Coller
                        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                            OutlinedButton(
                                onClick = { profileFilePicker.launch("*/*") },
                                modifier = Modifier.weight(1f),
                                shape = RoundedCornerShape(12.dp)
                            ) {
                                Column(
                                    horizontalAlignment = Alignment.CenterHorizontally,
                                    modifier = Modifier.padding(vertical = 8.dp)
                                ) {
                                    Icon(Icons.Outlined.AttachFile, null)
                                    Spacer(Modifier.height(4.dp))
                                    Text("Fichier", style = MaterialTheme.typography.labelMedium)
                                    Text("PDF, DOCX, TXT…", style = MaterialTheme.typography.labelSmall,
                                        color = MaterialTheme.colorScheme.onSurfaceVariant)
                                }
                            }
                            OutlinedButton(
                                onClick = { showProfileTextDialog = true },
                                modifier = Modifier.weight(1f),
                                shape = RoundedCornerShape(12.dp)
                            ) {
                                Column(
                                    horizontalAlignment = Alignment.CenterHorizontally,
                                    modifier = Modifier.padding(vertical = 8.dp)
                                ) {
                                    Icon(Icons.Outlined.ContentPaste, null)
                                    Spacer(Modifier.height(4.dp))
                                    Text("Coller", style = MaterialTheme.typography.labelMedium)
                                    Text("Copier-coller", style = MaterialTheme.typography.labelSmall,
                                        color = MaterialTheme.colorScheme.onSurfaceVariant)
                                }
                            }
                        }
                    }
                }

                Spacer(Modifier.height(24.dp))

                // ── Job Description ───────────────────────────────────────────
                SectionHeader(title = "Fiche de poste", badge = "Optionnel")
                Spacer(Modifier.height(10.dp))

                when (val jd = jobDescription) {
                    is JobDescription.None -> {
                        // Show two options
                        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                            // From file
                            OutlinedButton(
                                onClick = { jobDescPicker.launch("*/*") },
                                modifier = Modifier.weight(1f),
                                shape = RoundedCornerShape(12.dp)
                            ) {
                                Column(
                                    horizontalAlignment = Alignment.CenterHorizontally,
                                    modifier = Modifier.padding(vertical = 8.dp)
                                ) {
                                    Icon(Icons.Outlined.AttachFile, null)
                                    Spacer(Modifier.height(4.dp))
                                    Text("Fichier", style = MaterialTheme.typography.labelMedium)
                                    Text("PDF, DOCX, TXT…", style = MaterialTheme.typography.labelSmall,
                                        color = MaterialTheme.colorScheme.onSurfaceVariant)
                                }
                            }
                            // From text
                            OutlinedButton(
                                onClick = { showJobDescTextDialog = true },
                                modifier = Modifier.weight(1f),
                                shape = RoundedCornerShape(12.dp)
                            ) {
                                Column(
                                    horizontalAlignment = Alignment.CenterHorizontally,
                                    modifier = Modifier.padding(vertical = 8.dp)
                                ) {
                                    Icon(Icons.Outlined.ContentPaste, null)
                                    Spacer(Modifier.height(4.dp))
                                    Text("Coller", style = MaterialTheme.typography.labelMedium)
                                    Text("Copier-coller", style = MaterialTheme.typography.labelSmall,
                                        color = MaterialTheme.colorScheme.onSurfaceVariant)
                                }
                            }
                        }
                    }
                    is JobDescription.FromFile -> {
                        UploadCard(
                            title = "Fiche de poste",
                            subtitle = jd.fileName,
                            icon = Icons.Outlined.Description,
                            isLoaded = true,
                            loadedFileName = jd.fileName,
                            isLoading = jobDescLoading,
                            error = jobDescError,
                            onClick = {},
                            onClear = { viewModel.clearJobDescription() }
                        )
                    }
                    is JobDescription.FromText -> {
                        Card(
                            modifier = Modifier.fillMaxWidth(),
                            shape = RoundedCornerShape(16.dp),
                            colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceVariant),
                            border = BorderStroke(2.dp, Emerald)
                        ) {
                            Row(
                                modifier = Modifier.padding(16.dp),
                                verticalAlignment = Alignment.CenterVertically
                            ) {
                                Icon(Icons.Filled.CheckCircle, null, tint = Emerald, modifier = Modifier.size(24.dp))
                                Spacer(Modifier.width(12.dp))
                                Column(Modifier.weight(1f)) {
                                    Text("Fiche de poste saisie", style = MaterialTheme.typography.titleSmall)
                                    Text(
                                        jd.text.take(80) + if (jd.text.length > 80) "…" else "",
                                        style = MaterialTheme.typography.bodySmall,
                                        color = MaterialTheme.colorScheme.onSurfaceVariant
                                    )
                                }
                                IconButton(onClick = { viewModel.clearJobDescription() }) {
                                    Icon(Icons.Filled.Close, null)
                                }
                            }
                        }
                    }
                }

                Spacer(Modifier.height(32.dp))

                // ── Error state ───────────────────────────────────────────────
                AnimatedVisibility(generationState is GenerationState.Error) {
                    Card(
                        colors = CardDefaults.cardColors(
                            containerColor = MaterialTheme.colorScheme.errorContainer
                        ),
                        modifier = Modifier.fillMaxWidth()
                    ) {
                        Row(modifier = Modifier.padding(16.dp), verticalAlignment = Alignment.Top) {
                            Icon(Icons.Filled.Error, null, tint = MaterialTheme.colorScheme.error)
                            Spacer(Modifier.width(8.dp))
                            Text(
                                (generationState as? GenerationState.Error)?.message ?: "",
                                color = MaterialTheme.colorScheme.onErrorContainer,
                                style = MaterialTheme.typography.bodySmall
                            )
                        }
                    }
                    Spacer(Modifier.height(16.dp))
                }

                // ── Generate Button ───────────────────────────────────────────
                GenerateButton(
                    enabled = canGenerate,
                    isLoading = generationState is GenerationState.Loading,
                    onClick = { viewModel.generateCV() }
                )

                if (!canGenerate) {
                    Spacer(Modifier.height(8.dp))
                    Text(
                        text = when {
                            linkedInProfile == null -> "⬆ Importez ou collez d'abord votre profil"
                            else -> ""
                        },
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        textAlign = TextAlign.Center,
                        modifier = Modifier.fillMaxWidth()
                    )
                }

                Spacer(Modifier.height(40.dp))
            }
        }
    }

    // ── Dialogs ───────────────────────────────────────────────────────────────

    if (showSettingsDialog) {
        AiSettingsDialog(
            isPremium = (authState as? AuthState.Authenticated)?.user?.isPremium == true,
            selectedModel = selectedModel,
            onModelSelected = { viewModel.selectModel(it) },
            onSaveKey = { provider, key -> viewModel.saveApiKey(provider, key) },
            onDismiss = { showSettingsDialog = false },
            getApiKey = { provider -> viewModel.apiKeyForProvider(provider) }
        )
    }

    if (showJobDescTextDialog) {
        JobDescTextDialog(
            initialText = jobDescTextInput,
            onDismiss = { showJobDescTextDialog = false },
            onConfirm = { text ->
                jobDescTextInput = text
                viewModel.setJobDescriptionText(text)
                showJobDescTextDialog = false
            }
        )
    }

    if (showProfileTextDialog) {
        ProfileTextDialog(
            onDismiss = { showProfileTextDialog = false },
            onConfirm = { text ->
                viewModel.loadProfileText(text)
                showProfileTextDialog = false
            }
        )
    }
}

// ── Auth Section ──────────────────────────────────────────────────────────────

@Composable
private fun AuthSection(
    authState: AuthState,
    onSignIn: () -> Unit,
    onSignOut: () -> Unit
) {
    when (authState) {
        is AuthState.Authenticated -> {
            Card(
                shape = RoundedCornerShape(16.dp),
                colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceVariant)
            ) {
                Row(
                    modifier = Modifier.padding(16.dp).fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(12.dp)
                ) {
                    Box(
                        modifier = Modifier.size(40.dp).background(ElectricBlue, shape = RoundedCornerShape(50)),
                        contentAlignment = Alignment.Center
                    ) {
                        Text(
                            authState.user.displayName?.firstOrNull()?.toString() ?: "U",
                            color = Color.White, fontWeight = FontWeight.Bold
                        )
                    }
                    Column(Modifier.weight(1f)) {
                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            Text(authState.user.displayName ?: "Utilisateur", style = MaterialTheme.typography.titleSmall)
                            if (authState.user.isPremium) PremiumBadge()
                        }
                        Text(authState.user.email, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                    TextButton(onClick = onSignOut) { Text("Déconnexion") }
                }
            }

            if (authState.user.isPremium) {
                Spacer(Modifier.height(12.dp))
                InfoChip(
                    "Gemini Premium activé — clé sécurisée côté serveur",
                    Icons.Filled.CloudDone,
                    Emerald
                )
            }
        }
        is AuthState.Unauthenticated -> {
            OutlinedButton(
                onClick = onSignIn,
                modifier = Modifier.fillMaxWidth(),
                shape = RoundedCornerShape(12.dp)
            ) {
                Icon(Icons.Filled.AccountCircle, null, modifier = Modifier.size(20.dp))
                Spacer(Modifier.width(8.dp))
                Text("Connexion avec Google")
            }
            Spacer(Modifier.height(6.dp))
            Text(
                "Connectez-vous pour accéder à Gemini Premium ou utilisez votre clé API personnelle",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = TextAlign.Center,
                modifier = Modifier.fillMaxWidth()
            )
        }
        is AuthState.Loading -> {
            CircularProgressIndicator(modifier = Modifier.size(32.dp))
        }
    }
}

// ── API Key Section ───────────────────────────────────────────────────────────

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ModelPickerSection(
    selectedModel: AiModel,
    onConfigureClick: () -> Unit
) {
    Card(
        shape = RoundedCornerShape(16.dp),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceVariant
        ),
        modifier = Modifier.fillMaxWidth(),
        onClick = onConfigureClick
    ) {
        Row(
            modifier = Modifier.padding(16.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp)
        ) {
            Box(
                modifier = Modifier
                    .size(40.dp)
                    .clip(RoundedCornerShape(10.dp))
                    .background(ElectricBlue.copy(alpha = 0.15f)),
                contentAlignment = Alignment.Center
            ) {
                Icon(Icons.Filled.AutoAwesome, null, tint = ElectricBlue, modifier = Modifier.size(20.dp))
            }
            Column(Modifier.weight(1f)) {
                Text(selectedModel.displayName, style = MaterialTheme.typography.titleSmall)
                Text(
                    selectedModel.provider.displayName,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
            }
            Icon(Icons.Filled.ChevronRight, null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}

// ── Dialogs ───────────────────────────────────────────────────────────────────

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun AiSettingsDialog(
    selectedModel: AiModel,
    isPremium: Boolean,
    onModelSelected: (AiModel) -> Unit,
    onSaveKey: (AiProvider, String) -> Unit,
    onDismiss: () -> Unit,
    getApiKey: @Composable (AiProvider) -> StateFlow<String?>
) {
    // Onglet actif : provider sélectionné par défaut
    // If not premium, never start on GEMINI_PREMIUM
    val initialProvider = if (!isPremium && selectedModel.provider == AiProvider.GEMINI_PREMIUM)
        AiProvider.DEFAULT else selectedModel.provider
    var selectedProvider by remember { mutableStateOf(initialProvider) }
    val modelsForProvider = AiModels.forProvider(selectedProvider)

    // Clés API par provider (collectées en live)
    val keyAnthropic by getApiKey(AiProvider.ANTHROPIC).collectAsState()
    val keyOpenAI    by getApiKey(AiProvider.OPENAI).collectAsState()
    val keyGemini    by getApiKey(AiProvider.GEMINI).collectAsState()
    fun keyFor(p: AiProvider) = when (p) {
        AiProvider.ANTHROPIC -> keyAnthropic
        AiProvider.OPENAI    -> keyOpenAI
        AiProvider.GEMINI    -> keyGemini
        AiProvider.GEMINI_PREMIUM -> null  // no stored key — uses Google OAuth token
    }

    var keyInput by remember(selectedProvider) { mutableStateOf(keyFor(selectedProvider) ?: "") }
    var obscured by remember { mutableStateOf(true) }

    AlertDialog(
        onDismissRequest = onDismiss,
        icon = { Icon(Icons.Filled.AutoAwesome, null) },
        title = { Text("Modèle IA") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {

                // ── Sélection du provider ──────────────────────────────────────
                Text("Fournisseur", style = MaterialTheme.typography.labelMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant)

                // Row 1 — API providers (clé requise)
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    listOf(AiProvider.ANTHROPIC, AiProvider.OPENAI, AiProvider.GEMINI)
                        .forEach { provider ->
                        val selected = provider == selectedProvider
                        FilterChip(
                            selected = selected,
                            onClick = {
                                selectedProvider = provider
                                keyInput = keyFor(provider) ?: ""
                            },
                            label = {
                                Text(
                                    when (provider) {
                                        AiProvider.ANTHROPIC -> "Claude"
                                        AiProvider.OPENAI    -> "OpenAI"
                                        AiProvider.GEMINI    -> "Gemini"
                                        else                 -> provider.displayName
                                    },
                                    style = MaterialTheme.typography.labelSmall
                                )
                            },
                            modifier = Modifier.weight(1f)
                        )
                    }
                }

                // Row 2 — Gemini Premium (premium only)
                if (isPremium) {
                    HorizontalDivider(color = MaterialTheme.colorScheme.outline.copy(alpha = 0.3f))
                    FilterChip(
                        selected = selectedProvider == AiProvider.GEMINI_PREMIUM,
                        onClick = {
                            selectedProvider = AiProvider.GEMINI_PREMIUM
                            keyInput = ""
                            // Auto-select Gemini 2.0 Flash for premium
                            onModelSelected(AiModels.VERTEX_GEMINI_2_0_FLASH)
                        },
                        label = {
                            Row(
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.spacedBy(6.dp)
                            ) {
                                Icon(Icons.Filled.Star, null, tint = Gold, modifier = Modifier.size(12.dp))
                                Text("Gemini Premium (Recommandé)", style = MaterialTheme.typography.labelSmall)
                            }
                        },
                        modifier = Modifier.fillMaxWidth()
                    )
                } else {
                    Surface(
                        shape = RoundedCornerShape(8.dp),
                        color = Gold.copy(alpha = 0.08f),
                        border = androidx.compose.foundation.BorderStroke(1.dp, Gold.copy(alpha = 0.3f)),
                        modifier = Modifier.fillMaxWidth()
                    ) {
                        Row(
                            modifier = Modifier.padding(10.dp),
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(8.dp)
                        ) {
                            Icon(Icons.Filled.Lock, null, tint = Gold, modifier = Modifier.size(14.dp))
                            Text(
                                "Gemini Premium disponible pour les membres",
                                style = MaterialTheme.typography.labelSmall,
                                color = Gold
                            )
                        }
                    }
                }

                // ── Sélection du modèle ────────────────────────────────────────
                Text("Modèle", style = MaterialTheme.typography.labelMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant)

                Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    modelsForProvider.forEach { model ->
                        val selected = model == selectedModel && model.provider == selectedProvider
                        Surface(
                            shape = RoundedCornerShape(10.dp),
                            color = if (selected) ElectricBlue.copy(alpha = 0.12f)
                                    else MaterialTheme.colorScheme.surfaceVariant,
                            border = if (selected) androidx.compose.foundation.BorderStroke(
                                1.5.dp, ElectricBlue) else null,
                            modifier = Modifier
                                .fillMaxWidth()
                                .clickable { onModelSelected(model) }
                        ) {
                            Row(
                                modifier = Modifier.padding(horizontal = 12.dp, vertical = 10.dp),
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.spacedBy(10.dp)
                            ) {
                                if (selected) {
                                    Icon(Icons.Filled.CheckCircle, null,
                                        tint = ElectricBlue, modifier = Modifier.size(16.dp))
                                } else {
                                    Spacer(Modifier.size(16.dp))
                                }
                                Column {
                                    Text(model.displayName, style = MaterialTheme.typography.titleSmall)
                                    Text(model.description, style = MaterialTheme.typography.bodySmall,
                                        color = MaterialTheme.colorScheme.onSurfaceVariant)
                                }
                            }
                        }
                    }
                }

                // ── Clé API (masquée pour Gemini Premium — auth automatique) ─────────
                if (selectedProvider == AiProvider.GEMINI_PREMIUM) {
                    Surface(
                        shape = RoundedCornerShape(8.dp),
                        color = Emerald.copy(alpha = 0.08f),
                        border = androidx.compose.foundation.BorderStroke(1.dp, Emerald.copy(alpha = 0.3f)),
                        modifier = Modifier.fillMaxWidth()
                    ) {
                        Row(
                            modifier = Modifier.padding(12.dp),
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(8.dp)
                        ) {
                            Icon(Icons.Filled.CheckCircle, null, tint = Emerald, modifier = Modifier.size(16.dp))
                            Column {
                                Text(
                                    "Authentification automatique",
                                    style = MaterialTheme.typography.titleSmall,
                                    color = Emerald
                                )
                                Text(
                                    "Votre compte Google premium est utilisé. Aucune clé requise.",
                                    style = MaterialTheme.typography.bodySmall,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant
                                )
                            }
                        }
                    }
                } else {
                    Text(selectedProvider.keyLabel, style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant)

                    OutlinedTextField(
                        value = keyInput,
                        onValueChange = { keyInput = it },
                        label = { Text(selectedProvider.keyHint) },
                        singleLine = true,
                        visualTransformation = if (obscured)
                            androidx.compose.ui.text.input.PasswordVisualTransformation()
                        else androidx.compose.ui.text.input.VisualTransformation.None,
                        trailingIcon = {
                            IconButton(onClick = { obscured = !obscured }) {
                                Icon(if (obscured) Icons.Filled.Visibility else Icons.Filled.VisibilityOff, null)
                            }
                        },
                        supportingText = {
                            Text("Obtenez votre clé sur ${selectedProvider.keyUrl}",
                                style = MaterialTheme.typography.labelSmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant)
                        },
                        modifier = Modifier.fillMaxWidth()
                    )
                }
            }
        },
        confirmButton = {
            Button(
                onClick = {
                    if (selectedProvider != AiProvider.GEMINI_PREMIUM && keyInput.isNotBlank()) {
                        onSaveKey(selectedProvider, keyInput.trim())
                    }
                    onDismiss()
                }
            ) { Text("Enregistrer") }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text("Fermer") }
        }
    )
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ProfileTextDialog(
    onDismiss: () -> Unit,
    onConfirm: (String) -> Unit
) {
    var text by remember { mutableStateOf("") }

    AlertDialog(
        onDismissRequest = onDismiss,
        icon = { Icon(Icons.Outlined.ContentPaste, null) },
        title = { Text("Coller votre profil") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(
                    "Copiez-collez le texte de votre CV, profil LinkedIn ou tout autre texte décrivant votre parcours.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
                OutlinedTextField(
                    value = text,
                    onValueChange = { text = it },
                    label = { Text("Votre profil / CV…") },
                    modifier = Modifier.fillMaxWidth().height(250.dp),
                    maxLines = 20
                )
            }
        },
        confirmButton = {
            Button(onClick = { onConfirm(text) }, enabled = text.isNotBlank()) {
                Text("Confirmer")
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text("Annuler") }
        }
    )
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun JobDescTextDialog(
    initialText: String,
    onDismiss: () -> Unit,
    onConfirm: (String) -> Unit
) {
    var text by remember { mutableStateOf(initialText) }

    AlertDialog(
        onDismissRequest = onDismiss,
        icon = { Icon(Icons.Outlined.ContentPaste, null) },
        title = { Text("Coller la fiche de poste") },
        text = {
            OutlinedTextField(
                value = text,
                onValueChange = { text = it },
                label = { Text("Collez ici le texte de l'offre d'emploi") },
                modifier = Modifier.fillMaxWidth().height(250.dp),
                maxLines = 20
            )
        },
        confirmButton = {
            Button(onClick = { onConfirm(text) }, enabled = text.isNotBlank()) {
                Text("Confirmer")
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text("Annuler") }
        }
    )
}
