package com.stellasecret.cvgenerator.data.repository

import com.stellasecret.cvgenerator.util.CryptoUtils
import com.google.firebase.remoteconfig.FirebaseRemoteConfig
import com.google.firebase.remoteconfig.FirebaseRemoteConfigSettings
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.tasks.await
import kotlinx.coroutines.withContext
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class RemoteConfigRepository @Inject constructor() {

    companion object {
        private const val KEY_GEMINI_ENCRYPTED = "GEMINI_KEY_ENCRYPTED"
        // Cache TTL: 1 hour in production, 0 for debug
        private const val FETCH_INTERVAL_SECONDS = 3600L
    }

    private val remoteConfig: FirebaseRemoteConfig by lazy {
        FirebaseRemoteConfig.getInstance().apply {
            val settings = FirebaseRemoteConfigSettings.Builder()
                .setMinimumFetchIntervalInSeconds(FETCH_INTERVAL_SECONDS)
                .build()
            setConfigSettingsAsync(settings)
            // Default: empty string — app will show "configure key" message
            setDefaultsAsync(mapOf(KEY_GEMINI_ENCRYPTED to ""))
        }
    }

    /**
     * Fetches Remote Config, then decrypts and returns the Gemini API key.
     * Returns null if:
     *   - Remote Config fetch fails (network issue)
     *   - The encrypted value is empty or missing
     *   - Decryption fails (wrong AES key or tampered value)
     */
    suspend fun getGeminiApiKey(): String? = withContext(Dispatchers.IO) {
        try {
            // Fetch & activate (uses cache if within FETCH_INTERVAL_SECONDS)
            val success = remoteConfig.fetchAndActivate().await()
            android.util.Log.d("RemoteConfig", "Fetch successful: $success")

            val encryptedKey = remoteConfig.getString(KEY_GEMINI_ENCRYPTED)
            if (encryptedKey.isBlank()) {
                android.util.Log.e("RemoteConfig", "Encrypted key is blank in Remote Config")
                return@withContext null
            }

            // Decrypt with the AES key injected at build time via BuildConfig
            val decrypted = CryptoUtils.decryptGeminiKey(encryptedKey)
            if (decrypted == null) {
                android.util.Log.e("RemoteConfig", "Decryption failed for key: ${encryptedKey.take(10)}...")
            }
            decrypted

        } catch (e: Exception) {
            android.util.Log.e("RemoteConfig", "Error fetching/decrypting Remote Config", e)
            // Network error — try using cached value
            try {
                val cached = remoteConfig.getString(KEY_GEMINI_ENCRYPTED)
                if (cached.isBlank()) {
                    android.util.Log.e("RemoteConfig", "Cached key is also blank")
                    null
                } else {
                    CryptoUtils.decryptGeminiKey(cached)
                }
            } catch (e2: Exception) {
                android.util.Log.e("RemoteConfig", "Cache fallback also failed", e2)
                null
            }
        }
    }
}
