package com.stellasecret.cvgenerator.data.repository

import android.content.Context
import com.stellasecret.cvgenerator.BuildConfig
import com.stellasecret.cvgenerator.data.model.User
import com.google.android.gms.auth.api.signin.GoogleSignIn
import com.google.android.gms.auth.api.signin.GoogleSignInAccount
import com.google.android.gms.auth.api.signin.GoogleSignInClient
import com.google.android.gms.auth.api.signin.GoogleSignInOptions
import com.google.firebase.auth.FirebaseAuth
import com.google.firebase.auth.GoogleAuthProvider
import dagger.hilt.android.qualifiers.ApplicationContext
import kotlinx.coroutines.tasks.await
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class AuthRepository @Inject constructor(
    @ApplicationContext private val context: Context,
    private val firebaseAuth: FirebaseAuth
) {
    // ── Premium email list ────────────────────────────────────────────────────
    // Injected at build time via BuildConfig.PREMIUM_EMAILS (CI secret PREMIUM_EMAILS).
    // Format: comma-separated lowercase emails  e.g. "alice@example.com,bob@example.com"
    private val premiumEmails: Set<String> by lazy {
        BuildConfig.PREMIUM_EMAILS
            .split(",")
            .map { it.trim().lowercase() }
            .filter { it.isNotEmpty() }
            .toSet()
    }

    val googleSignInClient: GoogleSignInClient by lazy {
        val gso = GoogleSignInOptions.Builder(GoogleSignInOptions.DEFAULT_SIGN_IN)
            // OAUTH_WEB_CLIENT_ID is injected at build time from CI secret GOOGLE_WEB_CLIENT_ID
            .requestIdToken(BuildConfig.OAUTH_WEB_CLIENT_ID)
            .requestEmail()
            .build()
        GoogleSignIn.getClient(context, gso)
    }

    fun getCurrentUser(): User? {
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

    fun signOut() {
        firebaseAuth.signOut()
        googleSignInClient.signOut()
    }
}
