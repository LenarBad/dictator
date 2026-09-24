package io.lenar.dictator.tile

import android.app.Dialog
import android.app.PendingIntent
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.ServiceConnection
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.RemoteException
import android.service.quicksettings.Tile
import android.service.quicksettings.TileService
import android.view.ContextThemeWrapper
import android.view.LayoutInflater
import android.widget.ProgressBar
import android.widget.TextView
import android.widget.Toast
import androidx.appcompat.app.AlertDialog
import androidx.core.app.PendingIntentCompat
import com.google.android.material.button.MaterialButton
import io.lenar.dictator.R
import io.lenar.dictator.settings.SetupActivity
import io.lenar.dictator.settings.SetupGate
import io.lenar.dictator.stt.ISttCallback
import io.lenar.dictator.stt.ISttService
import io.lenar.dictator.stt.SttContract
import io.lenar.dictator.stt.SttService

/**
 * QS tile. Always clickable (never [Tile.STATE_UNAVAILABLE] — that swallows taps).
 *
 * Recording UI is a [showDialog]: more reliable than starting an Activity from the
 * shade on OEM skins, and counts as visible UI for microphone FGS on Android 14+.
 */
class DictatorTileService : TileService() {
    private var session: SessionController? = null
    private var clickBusy = false

    override fun onStartListening() {
        super.onStartListening()
        if (session != null) {
            refreshTile(SttContract.STATUS_RECORDING)
            return
        }
        if (!SetupGate.isVoiceReady(this)) {
            refreshTile(SttContract.STATUS_IDLE)
            return
        }
        withService { api -> refreshTile(api.status()) }
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
        session?.release()
        session = null
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

        if (session != null) {
            session?.requestStop()
            return
        }
        if (clickBusy) return
        clickBusy = true
        withService { api ->
            clickBusy = false
            when (api.status()) {
                SttContract.STATUS_RECORDING -> {
                    api.stop()
                    refreshTile(SttContract.STATUS_TRANSCRIBING)
                }
                SttContract.STATUS_TRANSCRIBING -> refreshTile(SttContract.STATUS_TRANSCRIBING)
                else -> openSessionDialog()
            }
        }
    }

    private fun openSessionDialog() {
        try {
            val controller =
                SessionController(this) {
                    session = null
                    if (SetupGate.isVoiceReady(this)) {
                        withService { api -> refreshTile(api.status()) }
                    } else {
                        refreshTile(SttContract.STATUS_IDLE)
                    }
                }
            session = controller
            showDialog(controller.dialog)
            controller.attach()
            refreshTile(SttContract.STATUS_RECORDING)
        } catch (err: Exception) {
            session = null
            Toast.makeText(
                applicationContext,
                err.message ?: getString(R.string.ime_stt_unavailable),
                Toast.LENGTH_LONG,
            ).show()
            // Last resort: activity via PendingIntent (API 34+) or Intent.
            openSessionActivityFallback()
        }
    }

    private fun openSetup() {
        val setup = Intent(this, SetupActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        openActivityAndCollapse(setup)
    }

    private fun openSessionActivityFallback() {
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

    private fun withService(block: (ISttService) -> Unit) {
        val connection =
            object : ServiceConnection {
                override fun onServiceConnected(name: ComponentName?, service: IBinder?) {
                    val api = ISttService.Stub.asInterface(service)
                    try {
                        block(api)
                    } catch (_: RemoteException) {
                    } finally {
                        try {
                            unbindService(this)
                        } catch (_: IllegalArgumentException) {
                        }
                    }
                }

                override fun onServiceDisconnected(name: ComponentName?) = Unit
            }
        try {
            bindService(Intent(this, SttService::class.java), connection, Context.BIND_AUTO_CREATE)
        } catch (_: Exception) {
            clickBusy = false
        }
    }

    /**
     * Dialog + AIDL session owned by the tile. [showDialog] collapses the shade
     * and gives while-in-use eligibility for the mic FGS.
     */
    private class SessionController(
        private val tile: DictatorTileService,
        private val onClosed: () -> Unit,
    ) {
        private val mainHandler = Handler(Looper.getMainLooper())
        private var stt: ISttService? = null
        private var bound = false
        private var started = false
        private var closed = false
        private var detachable = false

        private val txtStatus: TextView
        private val levelBar: ProgressBar
        private val btnStop: MaterialButton
        private val btnCollapse: MaterialButton
        val dialog: Dialog

        private val sttCallback =
            object : ISttCallback.Stub() {
                override fun onStatus(status: Int) {
                    mainHandler.post {
                        when (status) {
                            SttContract.STATUS_RECORDING -> {
                                txtStatus.setText(R.string.tile_session_recording)
                                btnStop.isEnabled = true
                                btnCollapse.isEnabled = detachable
                            }
                            SttContract.STATUS_TRANSCRIBING -> {
                                txtStatus.setText(R.string.tile_session_transcribing)
                                btnStop.isEnabled = false
                                btnCollapse.isEnabled = false
                            }
                        }
                    }
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
                        Toast.makeText(
                            tile.applicationContext,
                            if (body.isNotEmpty()) {
                                R.string.tile_copied_toast
                            } else {
                                R.string.notif_empty_body
                            },
                            Toast.LENGTH_SHORT,
                        ).show()
                        close()
                    }
                }

                override fun onError(message: String?) {
                    mainHandler.post {
                        Toast.makeText(
                            tile.applicationContext,
                            message ?: tile.getString(R.string.ime_stt_unavailable),
                            Toast.LENGTH_SHORT,
                        ).show()
                        close()
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
                            try {
                                SttService.ensureStarted(tile)
                                detachable = true
                            } catch (_: Exception) {
                                detachable = false
                            }
                            api.start(SttContract.SOURCE_TILE)
                        }
                    } catch (err: RemoteException) {
                        Toast.makeText(
                            tile.applicationContext,
                            err.message ?: tile.getString(R.string.ime_stt_unavailable),
                            Toast.LENGTH_SHORT,
                        ).show()
                        close()
                    }
                }

                override fun onServiceDisconnected(name: ComponentName?) {
                    stt = null
                    bound = false
                }
            }

        init {
            val themed = ContextThemeWrapper(tile, R.style.Theme_Dictator_TileSession)
            val view =
                LayoutInflater.from(themed).inflate(R.layout.activity_tile_session, null, false)
            txtStatus = view.findViewById(R.id.txtStatus)
            levelBar = view.findViewById(R.id.levelBar)
            btnStop = view.findViewById(R.id.btnStop)
            btnCollapse = view.findViewById(R.id.btnCollapse)
            btnStop.setOnClickListener { requestStop() }
            btnCollapse.setOnClickListener { collapse() }

            dialog =
                Dialog(themed, R.style.Theme_Dictator_TileSession).apply {
                    setContentView(view)
                    setCancelable(true)
                    setOnCancelListener { requestStop() }
                    setOnDismissListener {
                        release()
                        onClosed()
                    }
                }

        }

        fun attach() {
            tile.bindService(
                Intent(tile, SttService::class.java),
                connection,
                Context.BIND_AUTO_CREATE,
            )
        }

        private fun collapse() {
            if (!detachable || closed) return
            closed = true
            btnCollapse.isEnabled = false
            if (dialog.isShowing) {
                dialog.dismiss()
            } else {
                release()
                onClosed()
            }
        }

        fun requestStop() {
            btnStop.isEnabled = false
            btnCollapse.isEnabled = false
            txtStatus.setText(R.string.tile_session_transcribing)
            try {
                val api = stt
                if (api == null) {
                    close()
                } else {
                    api.stop()
                }
            } catch (_: RemoteException) {
                close()
            }
        }

        fun release() {
            if (bound) {
                try {
                    tile.unbindService(connection)
                } catch (_: IllegalArgumentException) {
                }
                bound = false
            }
            stt = null
        }

        private fun close() {
            if (closed) return
            closed = true
            if (dialog.isShowing) {
                dialog.dismiss()
            } else {
                release()
                onClosed()
            }
        }
    }
}
