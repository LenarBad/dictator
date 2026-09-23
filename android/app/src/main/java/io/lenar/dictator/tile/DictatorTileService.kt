package io.lenar.dictator.tile

import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.ServiceConnection
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.RemoteException
import android.service.quicksettings.Tile
import android.service.quicksettings.TileService
import android.widget.Toast
import io.lenar.dictator.R
import io.lenar.dictator.settings.SetupActivity
import io.lenar.dictator.settings.SetupGate
import io.lenar.dictator.stt.ISttCallback
import io.lenar.dictator.stt.ISttService
import io.lenar.dictator.stt.SttContract
import io.lenar.dictator.stt.SttService

/**
 * Quick Settings stand-in for a global hotkey. Delivers text to the clipboard only
 * (no [android.view.inputmethod.InputConnection] outside the IME).
 */
class DictatorTileService : TileService() {
    private val mainHandler = Handler(Looper.getMainLooper())
    private var stt: ISttService? = null
    private var bound = false
    private var status = SttContract.STATUS_IDLE

    private val sttCallback =
        object : ISttCallback.Stub() {
            override fun onStatus(newStatus: Int) {
                mainHandler.post {
                    status = newStatus
                    refreshTile()
                }
            }

            override fun onLevel(level: Float) = Unit

            override fun onResult(source: Int, text: String?) {
                if (source != SttContract.SOURCE_TILE) return
                mainHandler.post {
                    status = SttContract.STATUS_IDLE
                    refreshTile()
                    val body = text.orEmpty()
                    if (body.isNotEmpty()) {
                        Toast.makeText(
                            applicationContext,
                            getString(R.string.tile_copied_toast),
                            Toast.LENGTH_SHORT,
                        ).show()
                    }
                }
            }

            override fun onError(message: String?) {
                mainHandler.post {
                    status = SttContract.STATUS_IDLE
                    refreshTile()
                    Toast.makeText(
                        applicationContext,
                        message ?: getString(R.string.ime_stt_unavailable),
                        Toast.LENGTH_SHORT,
                    ).show()
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
                    status = api.status()
                } catch (_: RemoteException) {
                    stt = null
                }
                refreshTile()
            }

            override fun onServiceDisconnected(name: ComponentName?) {
                stt = null
                bound = false
                status = SttContract.STATUS_IDLE
                refreshTile()
            }
        }

    override fun onStartListening() {
        super.onStartListening()
        refreshTile()
        if (SetupGate.isVoiceReady(this)) {
            bindStt()
        }
    }

    override fun onStopListening() {
        super.onStopListening()
        // Keep the bind while a session is active; otherwise release.
        if (status == SttContract.STATUS_IDLE) {
            unbindStt()
        }
    }

    override fun onClick() {
        super.onClick()
        if (!SetupGate.isVoiceReady(this)) {
            Toast.makeText(this, R.string.tile_setup_required, Toast.LENGTH_SHORT).show()
            val setup = Intent(this, SetupActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            @Suppress("DEPRECATION")
            startActivityAndCollapse(setup)
            return
        }
        unlockAndRun {
            toggleSession()
        }
    }

    override fun onDestroy() {
        unbindStt()
        super.onDestroy()
    }

    private fun toggleSession() {
        val api = stt
        if (api == null) {
            bindStt()
            Toast.makeText(this, R.string.ime_stt_unavailable, Toast.LENGTH_SHORT).show()
            return
        }
        try {
            if (status == SttContract.STATUS_RECORDING) {
                api.stop()
            } else if (status == SttContract.STATUS_IDLE) {
                api.start(SttContract.SOURCE_TILE)
            }
        } catch (_: RemoteException) {
            Toast.makeText(this, R.string.ime_stt_unavailable, Toast.LENGTH_SHORT).show()
        }
    }

    private fun bindStt() {
        if (bound) return
        bindService(Intent(this, SttService::class.java), connection, Context.BIND_AUTO_CREATE)
    }

    private fun unbindStt() {
        if (!bound) return
        try {
            unbindService(connection)
        } catch (_: IllegalArgumentException) {
        }
        bound = false
        stt = null
    }

    private fun refreshTile() {
        val tile = qsTile ?: return
        if (!SetupGate.isVoiceReady(this)) {
            tile.state = Tile.STATE_UNAVAILABLE
            tile.label = getString(R.string.tile_label)
            tile.subtitle = getString(R.string.tile_subtitle_setup)
            tile.updateTile()
            return
        }
        when (status) {
            SttContract.STATUS_RECORDING -> {
                tile.state = Tile.STATE_ACTIVE
                tile.subtitle = getString(R.string.tile_subtitle_recording)
            }
            SttContract.STATUS_TRANSCRIBING -> {
                tile.state = Tile.STATE_ACTIVE
                tile.subtitle = getString(R.string.tile_subtitle_transcribing)
            }
            else -> {
                tile.state = Tile.STATE_INACTIVE
                tile.subtitle = getString(R.string.tile_subtitle_idle)
            }
        }
        tile.label = getString(R.string.tile_label)
        tile.updateTile()
    }
}
