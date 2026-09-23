package io.lenar.dictator.settings

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import android.view.View
import android.view.inputmethod.InputMethodManager
import android.widget.TextView
import androidx.activity.result.contract.ActivityResultContracts
import androidx.appcompat.app.AppCompatActivity
import androidx.core.content.ContextCompat
import androidx.appcompat.widget.SwitchCompat
import com.google.android.material.button.MaterialButton
import io.lenar.dictator.R

class SetupActivity : AppCompatActivity() {
    private lateinit var txtImeStatus: TextView
    private lateinit var txtMicStatus: TextView
    private lateinit var txtNotificationsStatus: TextView
    private lateinit var txtReady: TextView
    private lateinit var btnOpenImeSettings: MaterialButton
    private lateinit var btnGrantMic: MaterialButton
    private lateinit var btnGrantNotifications: MaterialButton
    private lateinit var labelNotifications: TextView
    private lateinit var switchReturnIme: SwitchCompat

    private val micPermission =
        registerForActivityResult(ActivityResultContracts.RequestPermission()) {
            refreshStatus()
        }

    private val notificationsPermission =
        registerForActivityResult(ActivityResultContracts.RequestPermission()) {
            refreshStatus()
        }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_setup)

        txtImeStatus = findViewById(R.id.txtImeStatus)
        txtMicStatus = findViewById(R.id.txtMicStatus)
        txtNotificationsStatus = findViewById(R.id.txtNotificationsStatus)
        txtReady = findViewById(R.id.txtReady)
        btnOpenImeSettings = findViewById(R.id.btnOpenImeSettings)
        btnGrantMic = findViewById(R.id.btnGrantMic)
        btnGrantNotifications = findViewById(R.id.btnGrantNotifications)
        labelNotifications = findViewById(R.id.labelNotifications)
        switchReturnIme = findViewById(R.id.switchReturnIme)

        btnOpenImeSettings.setOnClickListener {
            startActivity(Intent(Settings.ACTION_INPUT_METHOD_SETTINGS))
        }
        btnGrantMic.setOnClickListener {
            micPermission.launch(Manifest.permission.RECORD_AUDIO)
        }
        btnGrantNotifications.setOnClickListener {
            if (Build.VERSION.SDK_INT >= 33) {
                notificationsPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
            }
        }

        switchReturnIme.isChecked = DictatorPrefs.returnToPreviousIme(this)
        switchReturnIme.setOnCheckedChangeListener { _, checked ->
            DictatorPrefs.setReturnToPreviousIme(this, checked)
        }

        if (Build.VERSION.SDK_INT < 33) {
            labelNotifications.visibility = View.GONE
            txtNotificationsStatus.visibility = View.GONE
            btnGrantNotifications.visibility = View.GONE
        }
    }

    override fun onResume() {
        super.onResume()
        refreshStatus()
    }

    private fun refreshStatus() {
        val imeOn = isDictatorImeEnabled()
        txtImeStatus.text =
            getString(if (imeOn) R.string.setup_ime_enabled else R.string.setup_ime_disabled)
        txtImeStatus.setTextColor(
            ContextCompat.getColor(this, if (imeOn) R.color.dictator_ok else R.color.dictator_warn),
        )

        val micOn = hasPermission(Manifest.permission.RECORD_AUDIO)
        txtMicStatus.text =
            getString(if (micOn) R.string.setup_mic_granted else R.string.setup_mic_denied)
        txtMicStatus.setTextColor(
            ContextCompat.getColor(this, if (micOn) R.color.dictator_ok else R.color.dictator_warn),
        )
        btnGrantMic.isEnabled = !micOn

        val notificationsOk =
            if (Build.VERSION.SDK_INT >= 33) {
                hasPermission(Manifest.permission.POST_NOTIFICATIONS)
            } else {
                true
            }
        if (Build.VERSION.SDK_INT >= 33) {
            txtNotificationsStatus.text =
                getString(
                    if (notificationsOk) {
                        R.string.setup_notifications_granted
                    } else {
                        R.string.setup_notifications_denied
                    },
                )
            txtNotificationsStatus.setTextColor(
                ContextCompat.getColor(
                    this,
                    if (notificationsOk) R.color.dictator_ok else R.color.dictator_warn,
                ),
            )
            btnGrantNotifications.isEnabled = !notificationsOk
        }

        val ready = imeOn && micOn && notificationsOk
        txtReady.visibility = if (ready) View.VISIBLE else View.GONE
        DictatorPrefs.setSetupComplete(this, ready)
    }

    private fun isDictatorImeEnabled(): Boolean {
        val imm = getSystemService(InputMethodManager::class.java) ?: return false
        return imm.enabledInputMethodList.any { it.packageName == packageName }
    }

    private fun hasPermission(permission: String): Boolean =
        ContextCompat.checkSelfPermission(this, permission) == PackageManager.PERMISSION_GRANTED
}
