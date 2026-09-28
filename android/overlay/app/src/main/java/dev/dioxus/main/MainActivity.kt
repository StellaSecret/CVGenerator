package dev.dioxus.main

import android.content.Intent
import android.os.Bundle
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.ActivityResultLauncher
import androidx.activity.result.contract.ActivityResultContracts
import com.stellasecret.cvgenerator.GoogleDriveHelper

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
        GoogleDriveHelper.init(this, signInLauncher)
    }
}
