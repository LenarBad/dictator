package io.lenar.dictator.tile

import android.app.PendingIntent
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.ServiceConnection
import android.os.Build
import android.os.IBinder
import android.os.RemoteException
import android.service.quicksettings.Tile
import android.service.quicksettings.TileService
import androidx.appcompat.app.AlertDialog
import androidx.core.app.PendingIntentCompat
import io.lenar.dictator.R
import io.lenar.dictator.settings.SetupActivity
import io.lenar.dictator.settings.SetupGate
import io.lenar.dictator.stt.ISttService
import io.lenar.dictator.stt.SttContract

/**
 * QS tile. Always clickable (never [Tile.STATE_UNAVAILABLE] — that swallows taps).
 *
 * A microphone FGS is allowed only while the app is in the foreground (Android 14+
 * while-in-use). [showDialog] does not do that, so a tap launches [TileSessionActivity]
 * synchronously inside [onClick] — the QS background-activity exemption ends when
 * [onClick] returns.
 */
class DictatorTileService : TileService() {
    private var knownStatus: Int = SttContract.STATUS_IDLE
    private var listeningApi: ISttService? = null
    private var listeningBound = false

    private val listeningConnection =
        object : ServiceConnection {
            override fun onServiceConnected(name: ComponentName?, service: IBinder?) {
                val api = ISttService.Stub.asInterface(service)
                listeningApi = api
                try {
                    knownStatus = api.status()
                    refreshTile(knownStatus)
                } catch (_: RemoteException) {
                    listeningApi = null
                }
            }

            override fun onServiceDisconnected(name: ComponentName?) {
                listeningApi = null
                knownStatus = SttContract.STATUS_IDLE
                refreshTile(knownStatus)
            }
        }

    override fun onStartListening() {
        super.onStartListening()
        if (!SetupGate.isVoiceReady(this)) {
            knownStatus = SttContract.STATUS_IDLE
            refreshTile(knownStatus)
            return
        }
        refreshTile(knownStatus)
        if (listeningBound) return
        try {
            bindService(
                Intent(this, SttService::class.java),
                listeningConnection,
                Context.BIND_AUTO_CREATE,
            )
            listeningBound = true
        } catch (_: Exception) {
            listeningBound = false
        }
    }

    override fun onStopListening() {
        unbindListening()
        super.onStopListening()
    }

    override fun onClick() {
        super.onClick()
        val action = Runnable { handleClick() }
        if (isLocked) {
            unlockAndRun(action)
        } else {
            action.run()
        }
    }

    override fun onDestroy() {
        unbindListening()
        super.onDestroy()
    }

    private fun handleClick() {
        if (!SetupGate.isVoiceReady(this)) {
            showDialog(
                AlertDialog.Builder(this)
                    .setTitle(R.string.app_name)
                    .setMessage(R.string.tile_setup_required)
                    .setPositiveButton(R.string.setup_open_ime_settings) { _, _ ->
                        openSetup()
                    }
                    .setNegativeButton(android.R.string.cancel, null)
                    .create(),
            )
            return
        }

        val api = listeningApi
        val status =
            try {
                api?.status()
            } catch (_: RemoteException) {
                null
            }
        when (status) {
            SttContract.STATUS_RECORDING -> {
                try {
                    api?.stop()
                } catch (_: RemoteException) {
                }
                knownStatus = SttContract.STATUS_TRANSCRIBING
                refreshTile(knownStatus)
            }
            SttContract.STATUS_TRANSCRIBING -> {
                knownStatus = SttContract.STATUS_TRANSCRIBING
                refreshTile(knownStatus)
            }
            else -> openSessionActivity()
        }
    }

    private fun openSetup() {
        val setup = Intent(this, SetupActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        openActivityAndCollapse(setup)
    }

    private fun openSessionActivity() {
        val session =
            Intent(this, TileSessionActivity::class.java)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP)
        openActivityAndCollapse(session)
    }

    private fun openActivityAndCollapse(intent: Intent) {
        if (Build.VERSION.SDK_INT >= 34) {
            val pending =
                PendingIntentCompat.getActivity(
                    this,
                    intent.component?.className.hashCode(),
                    intent,
                    PendingIntent.FLAG_UPDATE_CURRENT,
                    false,
                )
            if (pending != null) {
                startActivityAndCollapse(pending)
                return
            }
        }
        @Suppress("DEPRECATION")
        startActivityAndCollapse(intent)
    }

    private fun refreshTile(status: Int) {
        val tile = qsTile ?: return
        val busy =
            status == SttContract.STATUS_RECORDING || status == SttContract.STATUS_TRANSCRIBING
        // Never UNAVAILABLE: many OEMs drop onClick entirely in that state.
        tile.state = if (busy) Tile.STATE_ACTIVE else Tile.STATE_INACTIVE
        tile.label = getString(R.string.tile_label)
        tile.subtitle =
            when {
                !SetupGate.isVoiceReady(this) -> getString(R.string.tile_subtitle_setup)
                status == SttContract.STATUS_RECORDING -> getString(R.string.tile_subtitle_recording)
                status == SttContract.STATUS_TRANSCRIBING -> getString(R.string.tile_subtitle_transcribing)
                else -> getString(R.string.tile_subtitle_idle)
            }
        tile.updateTile()
    }

    private fun unbindListening() {
        if (!listeningBound) return
        listeningBound = false
        listeningApi = null
        try {
            unbindService(listeningConnection)
        } catch (_: IllegalArgumentException) {
        }
    }
}
