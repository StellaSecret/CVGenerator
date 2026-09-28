package com.stellasecret.cvgenerator

import android.accounts.Account
import android.app.Activity
import android.content.Context
import android.content.Intent
import android.util.Log
import androidx.activity.result.ActivityResultLauncher
import com.google.android.gms.auth.GoogleAuthUtil
import com.google.android.gms.auth.api.signin.GoogleSignIn
import com.google.android.gms.auth.api.signin.GoogleSignInAccount
import com.google.android.gms.auth.api.signin.GoogleSignInClient
import com.google.android.gms.auth.api.signin.GoogleSignInOptions
import com.google.android.gms.common.api.ApiException
import com.google.android.gms.common.api.Scope
import java.io.File

private const val TAG = "DriveHelper"

// Ported from the PeopleModeler project (same author, same
// stellasecret.com Google Cloud project), which has this working in
// production on Android. Adjusted only for CVGenerator's package name and
// scope (drive.appdata, same as this app's existing web Drive sync).
object GoogleDriveHelper {
    private var activity: Activity? = null
    private var client: GoogleSignInClient? = null
    private var launcher: ActivityResultLauncher<Intent>? = null

    fun init(
        act: Activity,
        signInLauncher: ActivityResultLauncher<Intent>,
    ) {
        activity = act
        launcher = signInLauncher
        Log.d(TAG, "init: calling nativeInit")
        nativeInit(act.filesDir.absolutePath)
        Log.d(TAG, "init: building GoogleSignInOptions")
        val options =
            GoogleSignInOptions
                .Builder(GoogleSignInOptions.DEFAULT_SIGN_IN)
                .requestScopes(Scope("https://www.googleapis.com/auth/drive.appdata"))
                .requestEmail()
                .build()
        client = GoogleSignIn.getClient(act, options)
        Log.d(TAG, "init: client created, ready for sign-in")
    }

    private external fun nativeInit(filesDir: String)

    @JvmStatic
    fun startSignIn() {
        Log.d(TAG, "startSignIn: launcher=${launcher != null}, client=${client != null}")
        launcher?.let { l ->
            client?.let { c ->
                Log.d(TAG, "startSignIn: launching sign-in intent")
                l.launch(c.signInIntent)
            }
        }
    }

    fun handleSignInResult(data: Intent?) {
        Log.d(TAG, "handleSignInResult: data=$data")
        val task = GoogleSignIn.getSignedInAccountFromIntent(data)
        try {
            val account: GoogleSignInAccount = task.getResult(ApiException::class.java)
            Log.d(TAG, "handleSignInResult: account=${account.email}")
            Thread {
                getTokenAndSave(account)
            }.start()
        } catch (e: ApiException) {
            Log.e(TAG, "handleSignInResult: sign-in failed: ${e.message}")
            saveError("Sign-in failed: ${e.message}")
        }
    }

    private fun getTokenAndSave(account: GoogleSignInAccount) {
        try {
            val ctx: Context =
                activity ?: run {
                    Log.e(TAG, "getTokenAndSave: activity is null")
                    return
                }
            val acct: Account =
                account.account ?: run {
                    Log.e(TAG, "getTokenAndSave: account is null")
                    return
                }
            Log.d(TAG, "getTokenAndSave: calling GoogleAuthUtil.getToken for ${acct.name}")
            val token: String = GoogleAuthUtil.getToken(ctx, acct, "oauth2:https://www.googleapis.com/auth/drive.appdata")
            Log.d(TAG, "getTokenAndSave: token received, length=${token.length}")
            if (token.isNotEmpty()) {
                saveToken(token)
            } else {
                Log.e(TAG, "getTokenAndSave: token is empty")
                saveError("Token is empty")
            }
        } catch (e: Exception) {
            Log.e(TAG, "getTokenAndSave: ${e.message}")
            saveError("Token error: ${e.message}")
        }
    }

    private fun saveToken(token: String) {
        // Filename must match auth::token_path() in services/auth.rs (the
        // Rust side just reads this same file back).
        val f = File(activity!!.filesDir, ".cv_drive_token")
        f.parentFile?.mkdirs()
        f.writeText(token)
        nativeOnTokenSaved()
    }

    private external fun nativeOnTokenSaved()

    private fun saveError(msg: String) {
        val f = File(activity!!.filesDir, ".cv_drive_token_error")
        f.parentFile?.mkdirs()
        f.writeText(msg)
    }
}
