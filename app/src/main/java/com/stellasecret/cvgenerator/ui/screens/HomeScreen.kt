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
import androidx.compose.runtime.*
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
    onNavigateToResult: (String) -> Unit
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
    val savedApiKey by viewModel.savedApiKey.collectAsState()

    var showApiKeyDialog by remember { mutableStateOf(false) }
    var showJobDescTextDialog by remember { mutableStateOf(false) }
    var jobDescTextInput by remember { mutableStateOf("") }

    // Collect snackbar messages
    LaunchedEffect(Unit) {
        viewModel.snackbarMessage.collectLatest {
            snackbarHostState.showSnackbar(it)
        }
    }

    // Navigate to result when CV is generated
    LaunchedEffect(generationState) {
        if (generationState is GenerationState.Success) {
            val html = (generationState as GenerationState.Success).cv.htmlContent
            onNavigateToResult(Uri.encode(html))
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

    // ── LinkedIn PDF picker ───────────────────────────────────────────────────
    val linkedInPicker = rememberLauncherForActivityResult(
        ActivityResultContracts.GetContent()
    ) { uri: Uri? ->
        uri?.let { viewModel.loadLinkedInPdf(it) }
    }

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
    val isPremium = (authState as? AuthState.Authenticated)?.user?.isPremium == true
    val hasApiKey = !savedApiKey.isNullOrBlank()
    val canGenerate = linkedInProfile != null && (isPremium || hasApiKey)

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
                if (!isPremium) {
                    ApiKeySection(
                        hasApiKey = hasApiKey,
                        onConfigureClick = { showApiKeyDialog = true }
                    )
                    Spacer(Modifier.height(24.dp))
                }

                // ── LinkedIn PDF ──────────────────────────────────────────────
                SectionHeader(title = "Profil LinkedIn", badge = "PDF requis")
                Spacer(Modifier.height(10.dp))
                UploadCard(
                    title = "Profil LinkedIn",
                    subtitle = "Exportez votre profil depuis LinkedIn et importez le PDF",
                    icon = Icons.Outlined.Person,
                    isLoaded = linkedInProfile != null,
                    loadedFileName = linkedInProfile?.let { "Profil chargé ✓" },
                    isLoading = linkedInLoading,
                    error = linkedInError,
                    onClick = { linkedInPicker.launch("application/pdf") },
                    onClear = { /* viewModel.clearLinkedIn() */ }
                )

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
                    onClick = { viewModel.generateCV(if (isPremium) null else savedApiKey) }
                )

                if (!canGenerate) {
                    Spacer(Modifier.height(8.dp))
                    Text(
                        text = when {
                            linkedInProfile == null -> "⬆ Importez d'abord votre profil LinkedIn"
                            !hasApiKey && !isPremium -> "⚙ Configurez votre clé API Anthropic pour continuer"
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

    if (showApiKeyDialog) {
        ApiKeyDialog(
            currentKey = savedApiKey ?: "",
            onDismiss = { showApiKeyDialog = false },
            onSave = { key ->
                viewModel.saveApiKey(key)
                showApiKeyDialog = false
            }
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
                    "Vertex AI activé — accès illimité",
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
                "Connectez-vous pour accéder à Vertex AI (comptes premium) ou utilisez votre clé API",
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
private fun ApiKeySection(hasApiKey: Boolean, onConfigureClick: () -> Unit) {
    Card(
        shape = RoundedCornerShape(16.dp),
        colors = CardDefaults.cardColors(
            containerColor = if (hasApiKey) Emerald.copy(alpha = 0.08f)
            else MaterialTheme.colorScheme.errorContainer.copy(alpha = 0.4f)
        ),
        modifier = Modifier.fillMaxWidth(),
        onClick = onConfigureClick
    ) {
        Row(
            modifier = Modifier.padding(16.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp)
        ) {
            Icon(
                if (hasApiKey) Icons.Filled.Key else Icons.Outlined.Key,
                null,
                tint = if (hasApiKey) Emerald else MaterialTheme.colorScheme.error,
                modifier = Modifier.size(22.dp)
            )
            Column(Modifier.weight(1f)) {
                Text(
                    if (hasApiKey) "Clé API configurée" else "Clé API requise",
                    style = MaterialTheme.typography.titleSmall,
                    color = if (hasApiKey) Emerald else MaterialTheme.colorScheme.error
                )
                Text(
                    if (hasApiKey) "Clé Anthropic enregistrée" else "Configurez votre clé API Anthropic",
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
private fun ApiKeyDialog(
    currentKey: String,
    onDismiss: () -> Unit,
    onSave: (String) -> Unit
) {
    var keyInput by remember { mutableStateOf(currentKey) }
    var obscured by remember { mutableStateOf(true) }

    AlertDialog(
        onDismissRequest = onDismiss,
        icon = { Icon(Icons.Filled.Key, null) },
        title = { Text("Clé API Anthropic") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(
                    "Obtenez votre clé sur console.anthropic.com",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
                OutlinedTextField(
                    value = keyInput,
                    onValueChange = { keyInput = it },
                    label = { Text("Votre clé API Anthropic") },
                    singleLine = true,
                    visualTransformation = if (obscured)
                        androidx.compose.ui.text.input.PasswordVisualTransformation()
                    else androidx.compose.ui.text.input.VisualTransformation.None,
                    trailingIcon = {
                        IconButton(onClick = { obscured = !obscured }) {
                            Icon(if (obscured) Icons.Filled.Visibility else Icons.Filled.VisibilityOff, null)
                        }
                    },
                    modifier = Modifier.fillMaxWidth()
                )
            }
        },
        confirmButton = {
            Button(onClick = { onSave(keyInput.trim()) }, enabled = keyInput.isNotBlank()) {
                Text("Enregistrer")
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
