package io.lenar.dictator.settings

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.view.inputmethod.InputMethodManager
import androidx.core.content.ContextCompat

/** Shared readiness checks for tile / IME / recognition. */
object SetupGate {
    fun isDictatorImeEnabled(context: Context): Boolean {
        val imm = context.getSystemService(InputMethodManager::class.java) ?: return false
        return imm.enabledInputMethodList.any { it.packageName == context.packageName }
    }

    fun hasMicPermission(context: Context): Boolean =
        ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) ==
            PackageManager.PERMISSION_GRANTED

    /** Tile stays unavailable until IME is enabled and mic is granted. */
    fun isVoiceReady(context: Context): Boolean =
        isDictatorImeEnabled(context) && hasMicPermission(context)
}
