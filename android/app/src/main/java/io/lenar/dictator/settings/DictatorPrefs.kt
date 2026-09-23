package io.lenar.dictator.settings

import android.content.Context

/** SharedPreferences for v1 settings from docs/ANDROID.md. */
object DictatorPrefs {
    private const val NAME = "dictator"

    const val KEY_RETURN_TO_PREVIOUS_IME = "return_to_previous_ime"
    const val KEY_PASTE_ENABLED = "paste_enabled"
    const val KEY_PRELOAD_MODEL = "preload_model"
    const val KEY_SETUP_COMPLETE = "setup_complete"

    private fun prefs(context: Context) =
        context.applicationContext.getSharedPreferences(NAME, Context.MODE_PRIVATE)

    fun returnToPreviousIme(context: Context): Boolean =
        prefs(context).getBoolean(KEY_RETURN_TO_PREVIOUS_IME, true)

    fun setReturnToPreviousIme(context: Context, value: Boolean) {
        prefs(context).edit().putBoolean(KEY_RETURN_TO_PREVIOUS_IME, value).apply()
    }

    fun pasteEnabled(context: Context): Boolean =
        prefs(context).getBoolean(KEY_PASTE_ENABLED, true)

    fun setPasteEnabled(context: Context, value: Boolean) {
        prefs(context).edit().putBoolean(KEY_PASTE_ENABLED, value).apply()
    }

    fun preloadModel(context: Context): Boolean =
        prefs(context).getBoolean(KEY_PRELOAD_MODEL, true)

    fun setSetupComplete(context: Context, value: Boolean) {
        prefs(context).edit().putBoolean(KEY_SETUP_COMPLETE, value).apply()
    }

    fun isSetupComplete(context: Context): Boolean =
        prefs(context).getBoolean(KEY_SETUP_COMPLETE, false)
}
