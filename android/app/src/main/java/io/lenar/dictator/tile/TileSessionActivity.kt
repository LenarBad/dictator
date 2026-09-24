package io.lenar.dictator.tile

import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.ServiceConnection
import android.os.Bundle
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.RemoteException
import android.view.WindowManager
import android.widget.ProgressBar
import android.widget.TextView
import android.widget.Toast
import androidx.appcompat.app.AppCompatActivity
import com.google.android.material.button.MaterialButton
import io.lenar.dictator.R
import io.lenar.dictator.settings.SetupGate
import io.lenar.dictator.stt.ISttCallback
import io.lenar.dictator.stt.ISttService
import io.lenar.dictator.stt.SttContract
import io.lenar.dictator.stt.SttService

/**
 * Visible session UI started from the QS tile.
 *
 * Android 14+ blocks microphone foreground services from a pure [TileService]
 * click (while-in-use permission). A real activity puts the app in foreground
 * so [:stt] can record.
 */
class TileSessionActivity : AppCompatActivity() {
    private val mainHandler = Handler(Looper.getMainLooper())
    private var stt: ISttService? = null
    private var bound = false
    private var started = false
    private var finishing = false

    private lateinit var txtStatus: TextView
    private lateinit var levelBar: ProgressBar
    private lateinit var btnStop: MaterialButton

    private val sttCallback =
        object : ISttCallback.Stub() {
            override fun onStatus(status: Int) {
                mainHandler.post { applyStatus(status) }
            }

            override fun onLevel(level: Float) {
                mainHandler.post {
                    levelBar.progress = (level * 100f).toInt().coerceIn(0, 100)
                }
            }

            override fun onResult(source: Int, text: String?) {
                if (source != SttContract.SOURCE_TILE) return
                mainHandler.post {
                    val body = text.orEmpty()
                    if (body.isNotEmpty()) {
                        Toast.makeText(
                            applicationContext,
                            R.string.tile_copied_toast,
                            Toast.LENGTH_SHORT,
                        ).show()
                    } else {
                        Toast.makeText(
                            applicationContext,
                            R.string.notif_empty_body,
                            Toast.LENGTH_SHORT,
                        ).show()
                    }
                    finishSession()
                }
            }

            override fun onError(message: String?) {
                mainHandler.post {
                    Toast.makeText(
                        applicationContext,
                        message ?: getString(R.string.ime_stt_unavailable),
                        Toast.LENGTH_SHORT,
                    ).show()
                    finishSession()
                }
            }
        }

    private val connection =
        object : ServiceConnection {
            override fun onServiceConnected(name: ComponentName?, service: IBinder?) {
                val api = ISttService.Stub.asInterface(service)
                stt = api
                bound = true
                try {
                    api.register(sttCallback)
                    if (!started) {
                        started = true
                        api.start(SttContract.SOURCE_TILE)
                    }
                } catch (_: RemoteException) {
                    Toast.makeText(
                        this@TileSessionActivity,
                        R.string.ime_stt_unavailable,
                        Toast.LENGTH_SHORT,
                    ).show()
                    finishSession()
                }
            }

            override fun onServiceDisconnected(name: ComponentName?) {
                stt = null
                bound = false
            }
        }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        setContentView(R.layout.activity_tile_session)

        txtStatus = findViewById(R.id.txtStatus)
        levelBar = findViewById(R.id.levelBar)
        btnStop = findViewById(R.id.btnStop)
        btnStop.setOnClickListener { requestStop() }

        if (!SetupGate.isVoiceReady(this)) {
            Toast.makeText(this, R.string.tile_setup_required, Toast.LENGTH_SHORT).show()
            finish()
            return
        }

        bindService(Intent(this, SttService::class.java), connection, Context.BIND_AUTO_CREATE)
    }

    override fun onDestroy() {
        if (bound) {
            try {
                unbindService(connection)
            } catch (_: IllegalArgumentException) {
            }
            bound = false
        }
        stt = null
        super.onDestroy()
    }

    @Deprecated("Deprecated in Java")
    override fun onBackPressed() {
        requestStop()
    }

    private fun requestStop() {
        btnStop.isEnabled = false
        txtStatus.setText(R.string.tile_session_transcribing)
        try {
            stt?.stop()
        } catch (_: RemoteException) {
            finishSession()
        }
    }

    private fun applyStatus(status: Int) {
        when (status) {
            SttContract.STATUS_RECORDING -> {
                txtStatus.setText(R.string.tile_session_recording)
                btnStop.isEnabled = true
            }
            SttContract.STATUS_TRANSCRIBING -> {
                txtStatus.setText(R.string.tile_session_transcribing)
                btnStop.isEnabled = false
            }
            else -> Unit
        }
    }

    private fun finishSession() {
        if (finishing) return
        finishing = true
        finish()
    }
}
