package dev.dioxus.main

import android.content.Intent
import android.os.Bundle
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.ActivityResultLauncher
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import com.stellasecret.cvgenerator.GoogleDriveHelper
import com.stellasecret.cvgenerator.PdfExporter

// Dioxus's generated Logger.kt (package dev.dioxus.main) refers to
// `BuildConfig` unqualified, expecting it in its own package. Our Gradle
// namespace is com.stellasecret.cvgenerator, so that's where AGP generates
// BuildConfig — bridge the two with a package-level alias.
typealias BuildConfig = com.stellasecret.cvgenerator.BuildConfig

// Overrides Dioxus's default-generated MainActivity so sign-in results can
// be routed back into GoogleDriveHelper. WryActivity itself is left as
// Dioxus generates it (a separate file in this same package, untouched by
// this overlay) — this only adds the sign-in launcher plumbing.
class MainActivity : WryActivity() {
    private lateinit var signInLauncher: ActivityResultLauncher<Intent>

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        signInLauncher =
            registerForActivityResult(
                ActivityResultContracts.StartActivityForResult(),
            ) { result ->
                GoogleDriveHelper.handleSignInResult(result.data)
            }
        super.onCreate(savedInstanceState)

        // Edge-to-edge is mandatory on targetSdk 35+, so the WebView would
        // otherwise be drawn underneath the status bar / navigation bar /
        // camera cutout (the sticky nav's top-left was hidden behind the
        // clock). Pad the content root by those insets, plus the keyboard,
        // so the whole web UI sits inside the visible area.
        val content = findViewById<android.view.View>(android.R.id.content)
        ViewCompat.setOnApplyWindowInsetsListener(content) { view, insets ->
            val bars = insets.getInsets(
                WindowInsetsCompat.Type.systemBars() or
                    WindowInsetsCompat.Type.displayCutout() or
                    WindowInsetsCompat.Type.ime(),
            )
            view.setPadding(bars.left, bars.top, bars.right, bars.bottom)
            WindowInsetsCompat.CONSUMED
        }
        ViewCompat.requestApplyInsets(content)

        GoogleDriveHelper.init(this, signInLauncher)
        PdfExporter.init(this)
    }
}
