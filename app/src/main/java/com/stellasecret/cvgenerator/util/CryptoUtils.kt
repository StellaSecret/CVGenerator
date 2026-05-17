package com.stellasecret.cvgenerator.util

import android.util.Base64
import com.stellasecret.cvgenerator.BuildConfig
import javax.crypto.Cipher
import javax.crypto.spec.GCMParameterSpec
import javax.crypto.spec.SecretKeySpec

/**
 * AES-256-GCM decryption utility.
 *
 * The AES key is stored as a CI secret (GEMINI_ENCRYPTION_KEY) and injected
 * into BuildConfig at build time — never in source code.
 *
 * Encrypted format: base64( nonce[12] | ciphertext | GCM-tag[16] )
 * This matches the output of encrypt_gemini_key.py.
 */
object CryptoUtils {

    private const val ALGORITHM  = "AES/GCM/NoPadding"
    private const val TAG_LENGTH = 128  // bits

    /**
     * Decrypts a base64-encoded AES-256-GCM payload.
     * Returns null if decryption fails (wrong key, tampered data, etc.)
     */
    fun decryptGeminiKey(encryptedB64: String): String? {
        return try {
            val aesKeyB64 = BuildConfig.GEMINI_ENCRYPTION_KEY
            if (aesKeyB64.isBlank()) return null

            val keyBytes  = Base64.decode(aesKeyB64, Base64.DEFAULT)
            val data      = Base64.decode(encryptedB64, Base64.DEFAULT)

            if (data.size < 12 + 16) return null   // nonce + min tag

            val nonce      = data.copyOfRange(0, 12)
            val ciphertext = data.copyOfRange(12, data.size)

            val secretKey = SecretKeySpec(keyBytes, "AES")
            val gcmSpec   = GCMParameterSpec(TAG_LENGTH, nonce)

            val cipher = Cipher.getInstance(ALGORITHM)
            cipher.init(Cipher.DECRYPT_MODE, secretKey, gcmSpec)

            String(cipher.doFinal(ciphertext), Charsets.UTF_8)
        } catch (e: Exception) {
            null  // wrong key, tampered ciphertext, or bad input
        }
    }
}
