package com.stellasecret.cvgenerator.ui.screens

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.print.PrintAttributes
import android.print.PrintManager
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.compose.animation.*
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.FileProvider
import com.stellasecret.cvgenerator.ui.theme.*
import java.io.File

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ResultScreen(
    encodedHtml: String,
    onBack: () -> Unit
) {
    val context = LocalContext.current
    val htmlContent = Uri.decode(encodedHtml)

    var webView by remember { mutableStateOf<WebView?>(null) }
    var isLoaded by remember { mutableStateOf(false) }
    var showShareMenu by remember { mutableStateOf(false) }

    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    Column {
                        Text("Votre CV", fontWeight = FontWeight.Bold)
                        Text(
                            "Généré par IA",
                            style = MaterialTheme.typography.labelSmall,
                            color = ElectricBlue
                        )
                    }
                },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, "Retour")
                    }
                },
                actions = {
                    // Print / Export PDF
                    IconButton(onClick = {
                        webView?.let { printWebView(context, it) }
                    }) {
                        Icon(Icons.Filled.Print, "Imprimer / PDF")
                    }
                    // Share
                    IconButton(onClick = { showShareMenu = true }) {
                        Icon(Icons.Filled.Share, "Partager")
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.surface
                )
            )
        },
        containerColor = MaterialTheme.colorScheme.background
    ) { paddingValues ->
        Box(
            modifier = Modifier
                .fillMaxSize()
                .padding(paddingValues)
        ) {
            // WebView renders the HTML CV
            AndroidView(
                factory = { ctx ->
                    WebView(ctx).apply {
                        settings.apply {
                            javaScriptEnabled = false
                            builtInZoomControls = true
                            displayZoomControls = false
                            useWideViewPort = true
                            loadWithOverviewMode = true
                        }
                        webViewClient = object : WebViewClient() {
                            override fun onPageFinished(view: WebView?, url: String?) {
                                isLoaded = true
                            }
                        }
                        setBackgroundColor(android.graphics.Color.WHITE)
                        loadDataWithBaseURL(null, htmlContent, "text/html", "UTF-8", null)
                    }.also { webView = it }
                },
                modifier = Modifier.fillMaxSize()
            )

            // Loading overlay
            AnimatedVisibility(
                visible = !isLoaded,
                exit = fadeOut()
            ) {
                Box(
                    modifier = Modifier
                        .fillMaxSize()
                        .background(MaterialTheme.colorScheme.background),
                    contentAlignment = Alignment.Center
                ) {
                    Column(
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(16.dp)
                    ) {
                        CircularProgressIndicator(color = ElectricBlue)
                        Text(
                            "Rendu du CV en cours…",
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant
                        )
                    }
                }
            }

            // Bottom action bar
            AnimatedVisibility(
                visible = isLoaded,
                modifier = Modifier.align(Alignment.BottomCenter),
                enter = slideInVertically { it } + fadeIn()
            ) {
                Surface(
                    shadowElevation = 8.dp,
                    color = MaterialTheme.colorScheme.surface,
                    modifier = Modifier.fillMaxWidth()
                ) {
                    Row(
                        modifier = Modifier
                            .padding(horizontal = 16.dp, vertical = 12.dp)
                            .navigationBarsPadding(),
                        horizontalArrangement = Arrangement.spacedBy(12.dp)
                    ) {
                        // Save as HTML
                        OutlinedButton(
                            onClick = { saveHtmlFile(context, htmlContent) },
                            modifier = Modifier.weight(1f),
                            shape = RoundedCornerShape(12.dp)
                        ) {
                            Icon(Icons.Filled.Download, null, modifier = Modifier.size(18.dp))
                            Spacer(Modifier.width(6.dp))
                            Text("HTML")
                        }

                        // Export PDF (via print)
                        Button(
                            onClick = { webView?.let { printWebView(context, it) } },
                            modifier = Modifier.weight(2f),
                            shape = RoundedCornerShape(12.dp),
                            colors = ButtonDefaults.buttonColors(containerColor = ElectricBlue)
                        ) {
                            Icon(Icons.Filled.PictureAsPdf, null, modifier = Modifier.size(18.dp))
                            Spacer(Modifier.width(6.dp))
                            Text("Exporter PDF", fontWeight = FontWeight.Bold)
                        }
                    }
                }
            }
        }
    }

    // Share bottom sheet
    if (showShareMenu) {
        ModalBottomSheet(onDismissRequest = { showShareMenu = false }) {
            ShareOptions(
                onShareHtml = {
                    showShareMenu = false
                    shareHtml(context, htmlContent)
                },
                onPrint = {
                    showShareMenu = false
                    webView?.let { printWebView(context, it) }
                },
                onDismiss = { showShareMenu = false }
            )
        }
    }
}

@Composable
private fun ShareOptions(
    onShareHtml: () -> Unit,
    onPrint: () -> Unit,
    onDismiss: () -> Unit
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp)
    ) {
        Text(
            "Partager le CV",
            style = MaterialTheme.typography.titleMedium,
            fontWeight = FontWeight.Bold
        )
        Spacer(Modifier.height(4.dp))

        ShareOptionItem(
            icon = Icons.Filled.Code,
            title = "Partager en HTML",
            subtitle = "Fichier HTML éditable",
            onClick = onShareHtml
        )
        ShareOptionItem(
            icon = Icons.Filled.Print,
            title = "Imprimer / Exporter PDF",
            subtitle = "Via le menu d'impression système",
            onClick = onPrint
        )

        Spacer(Modifier.height(8.dp))
        TextButton(onClick = onDismiss, modifier = Modifier.align(Alignment.CenterHorizontally)) {
            Text("Annuler")
        }
        Spacer(Modifier.height(16.dp))
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ShareOptionItem(
    icon: androidx.compose.ui.graphics.vector.ImageVector,
    title: String,
    subtitle: String,
    onClick: () -> Unit
) {
    Card(
        onClick = onClick,
        shape = RoundedCornerShape(12.dp),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceVariant),
        modifier = Modifier.fillMaxWidth()
    ) {
        Row(
            modifier = Modifier.padding(16.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(16.dp)
        ) {
            Icon(icon, null, tint = ElectricBlue, modifier = Modifier.size(24.dp))
            Column(Modifier.weight(1f)) {
                Text(title, style = MaterialTheme.typography.titleSmall)
                Text(subtitle, style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            Icon(Icons.Filled.ChevronRight, null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}

// ── Utility functions ─────────────────────────────────────────────────────────

private fun printWebView(context: Context, webView: WebView) {
    val printManager = context.getSystemService(Context.PRINT_SERVICE) as PrintManager
    val jobName = "CVGenerator_CV"
    val printAdapter = webView.createPrintDocumentAdapter(jobName)
    val printAttributes = PrintAttributes.Builder()
        .setMediaSize(PrintAttributes.MediaSize.ISO_A4)
        .setResolution(PrintAttributes.Resolution("pdf", "pdf", 600, 600))
        .setMinMargins(PrintAttributes.Margins.NO_MARGINS)
        .build()
    printManager.print(jobName, printAdapter, printAttributes)
}

private fun saveHtmlFile(context: Context, htmlContent: String) {
    try {
        val fileName = "cv_cvgenerator_${System.currentTimeMillis()}.html"
        val file = File(context.cacheDir, fileName)
        file.writeText(htmlContent)
        val uri = FileProvider.getUriForFile(context, "${context.packageName}.fileprovider", file)
        val intent = Intent(Intent.ACTION_VIEW).apply {
            setDataAndType(uri, "text/html")
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
        context.startActivity(Intent.createChooser(intent, "Ouvrir le CV"))
    } catch (e: Exception) {
        e.printStackTrace()
    }
}

private fun shareHtml(context: Context, htmlContent: String) {
    try {
        val fileName = "cv_cvgenerator.html"
        val file = File(context.cacheDir, fileName)
        file.writeText(htmlContent)
        val uri = FileProvider.getUriForFile(context, "${context.packageName}.fileprovider", file)
        val intent = Intent(Intent.ACTION_SEND).apply {
            type = "text/html"
            putExtra(Intent.EXTRA_STREAM, uri)
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
        context.startActivity(Intent.createChooser(intent, "Partager le CV"))
    } catch (e: Exception) {
        e.printStackTrace()
    }
}
