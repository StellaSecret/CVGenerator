package com.stellasecret.cvgenerator.data.repository

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import com.stellasecret.cvgenerator.data.model.AiModel
import com.stellasecret.cvgenerator.data.model.AiModels
import com.stellasecret.cvgenerator.data.model.AiProvider
import dagger.hilt.android.qualifiers.ApplicationContext
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import javax.inject.Inject
import javax.inject.Singleton

private val Context.dataStore: DataStore<Preferences> by preferencesDataStore(name = "cvgenerator_prefs")

@Singleton
class PreferencesRepository @Inject constructor(
    @ApplicationContext private val context: Context
) {
    // ── Keys ──────────────────────────────────────────────────────────────────
    private val SELECTED_MODEL_ID = stringPreferencesKey("selected_model_id")

    // Une clé par provider — l'utilisateur peut en avoir plusieurs configurées
    private val KEY_ANTHROPIC = stringPreferencesKey("api_key_anthropic")
    private val KEY_OPENAI    = stringPreferencesKey("api_key_openai")
    private val KEY_GEMINI    = stringPreferencesKey("api_key_gemini")

    // ── Flows ─────────────────────────────────────────────────────────────────
    val selectedModelFlow: Flow<AiModel> = context.dataStore.data.map { prefs ->
        AiModels.fromId(prefs[SELECTED_MODEL_ID] ?: AiModels.DEFAULT.id)
    }

    fun apiKeyFlow(provider: AiProvider): Flow<String?> =
        context.dataStore.data.map { prefs -> prefs[keyFor(provider)] }

    // ── Writes ────────────────────────────────────────────────────────────────
    suspend fun saveSelectedModel(model: AiModel) {
        context.dataStore.edit { prefs -> prefs[SELECTED_MODEL_ID] = model.id }
    }

    suspend fun saveApiKey(provider: AiProvider, apiKey: String) {
        context.dataStore.edit { prefs -> prefs[keyFor(provider)] = apiKey }
    }

    suspend fun clearApiKey(provider: AiProvider) {
        context.dataStore.edit { prefs -> prefs.remove(keyFor(provider)) }
    }

    // ── Helpers ───────────────────────────────────────────────────────────────
    private fun keyFor(provider: AiProvider): Preferences.Key<String> = when (provider) {
        AiProvider.ANTHROPIC -> KEY_ANTHROPIC
        AiProvider.OPENAI    -> KEY_OPENAI
        AiProvider.GEMINI    -> KEY_GEMINI
        AiProvider.GEMINI_PREMIUM -> KEY_GEMINI  // Gemini Premium uses Google OAuth or shared key
    }
}
