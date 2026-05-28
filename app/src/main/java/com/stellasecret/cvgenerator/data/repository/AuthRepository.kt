package com.stellasecret.cvgenerator.data.repository

import android.content.Context
import com.stellasecret.cvgenerator.BuildConfig
import com.stellasecret.cvgenerator.data.model.User
import com.google.android.gms.auth.api.signin.GoogleSignIn
import com.google.android.gms.auth.api.signin.GoogleSignInAccount
import com.google.android.gms.auth.api.signin.GoogleSignInClient
import com.google.android.gms.auth.api.signin.GoogleSignInOptions
import com.google.android.gms.common.api.Scope
import com.google.firebase.auth.FirebaseAuth
import com.google.firebase.auth.GoogleAuthProvider
import dagger.hilt.android.qualifiers.ApplicationContext
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.tasks.await
import kotlinx.coroutines.withContext
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class AuthRepository @Inject constructor(
    @ApplicationContext private val context: Context,
    private val firebaseAuth: FirebaseAuth
) {
    // ── Premium email list ────────────────────────────────────────────────────
    private val premiumEmails: Set<String> by lazy {
        BuildConfig.PREMIUM_EMAILS
            .split(",")
            .map { it.trim().lowercase() }
            .filter { it.isNotEmpty() }
            .toSet()
    }

    // ── Stored account (set on sign-in, used for Vertex AI token) ─────────────
    // GoogleSignInAccount contains the android.accounts.Account needed by GoogleAuthUtil
    @Volatile private var storedAccount: GoogleSignInAccount? = null

    // ── Google Sign-In client ─────────────────────────────────────────────────
    val googleSignInClient: GoogleSignInClient by lazy {
        val gso = GoogleSignInOptions.Builder(GoogleSignInOptions.DEFAULT_SIGN_IN)
            .requestIdToken(BuildConfig.OAUTH_WEB_CLIENT_ID)
            .requestEmail()
            // Request the Vertex AI scope so premium users can call the API
            .requestScopes(Scope("https://www.googleapis.com/auth/cloud-platform"))
            .build()
        GoogleSignIn.getClient(context, gso)
    }

    // ── Auth state ────────────────────────────────────────────────────────────

    fun getCurrentUser(): User? {
        // Restore stored account from last sign-in if app was restarted
        if (storedAccount == null) {
            storedAccount = GoogleSignIn.getLastSignedInAccount(context)
        }
        val firebaseUser = firebaseAuth.currentUser ?: return null
        val email = firebaseUser.email ?: return null
        return User(
            email = email,
            displayName = firebaseUser.displayName,
            photoUrl = firebaseUser.photoUrl?.toString(),
            isPremium = isPremiumEmail(email)
        )
    }

    fun isLoggedIn(): Boolean = firebaseAuth.currentUser != null

    fun isPremiumEmail(email: String): Boolean = email.lowercase() in premiumEmails

    suspend fun signInWithGoogle(account: GoogleSignInAccount): Result<User> {
        return try {
            // Store account immediately — needed for getVertexAiAccessToken()
            storedAccount = account

            val credential = GoogleAuthProvider.getCredential(account.idToken, null)
            val result = firebaseAuth.signInWithCredential(credential).await()
            val firebaseUser = result.user
                ?: return Result.failure(Exception("Sign-in failed: no user returned"))

            val email = firebaseUser.email ?: ""
            val user = User(
                email = email,
                displayName = firebaseUser.displayName,
                photoUrl = firebaseUser.photoUrl?.toString(),
                isPremium = isPremiumEmail(email)
            )
            Result.success(user)
        } catch (e: Exception) {
            Result.failure(e)
        }
    }

    // ── Premium access token ─────────────────────────────────────────────────
    // Returns a real OAuth2 Bearer token (ya29…) valid for Google Cloud REST API.
    // Uses the stored GoogleSignInAccount — NOT Firebase getIdToken() which returns
    // a JWT that Google Cloud rejects with 401 UNAUTHENTICATED.
    suspend fun getPremiumAccessToken(): String? = withContext(Dispatchers.IO) {
        try {
            // Prefer the in-memory stored account; fall back to last signed-in account
            val account = storedAccount
                ?: GoogleSignIn.getLastSignedInAccount(context)
                ?: return@withContext null

            val androidAccount = account.account
                ?: return@withContext null

            // GoogleAuthUtil.getToken() exchanges the stored Google credentials for
            // a short-lived OAuth2 access token for the specified scope.
            // This call may block briefly if the token needs refreshing.
            com.google.android.gms.auth.GoogleAuthUtil.getToken(
                context,
                androidAccount,
                "oauth2:https://www.googleapis.com/auth/cloud-platform"
            )
        } catch (e: com.google.android.gms.auth.UserRecoverableAuthException) {
            // Scope was not granted — user needs to re-authenticate
            // The calling code will show "Reconnectez-vous" message
            null
        } catch (e: Exception) {
            null
        }
    }

    fun signOut() {
        storedAccount = null
        firebaseAuth.signOut()
        googleSignInClient.signOut()
    }
}
