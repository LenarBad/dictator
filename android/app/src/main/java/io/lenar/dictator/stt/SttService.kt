package io.lenar.dictator.stt

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.RemoteCallbackList
import android.os.RemoteException
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import io.lenar.dictator.R
import io.lenar.dictator.settings.SetupActivity
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicInteger

/**
 * Mic + STT in process `:stt`. Clients (IME) bind via AIDL and never load ONNX.
 */
class SttService : Service() {
    private val mainHandler = Handler(Looper.getMainLooper())
    private val worker = Executors.newSingleThreadExecutor()
    private val callbacks = RemoteCallbackList<ISttCallback>()
    private lateinit var engine: Engine
    private val bindCount = AtomicInteger(0)

    @Volatile private var status: Int = SttContract.STATUS_IDLE
    @Volatile private var activeSource: Int = SttContract.SOURCE_IME
    private var recorder: AudioRecorder? = null

    private val binder =
        object : ISttService.Stub() {
            override fun register(callback: ISttCallback?) {
                if (callback == null) return
                callbacks.register(callback)
                try {
                    callback.onStatus(status)
                } catch (_: RemoteException) {
                }
            }

            override fun start(source: Int) {
                mainHandler.post { startSession(source) }
            }

            override fun stop() {
                mainHandler.post { stopSession() }
            }

            override fun preload() {
                worker.execute {
                    try {
                        engine.preload()
                    } catch (err: Exception) {
                        broadcastError(err.message ?: "preload failed")
                    }
                }
            }

            override fun status(): Int = status
        }

    override fun onCreate() {
        super.onCreate()
        engine = Engine(assets)
        if (io.lenar.dictator.settings.DictatorPrefs.preloadModel(this)) {
            worker.execute {
                try {
                    engine.preload()
                } catch (_: Exception) {
                    // First real utterance will surface the error.
                }
            }
        }
    }

    override fun onBind(intent: Intent?): IBinder {
        bindCount.incrementAndGet()
        return binder
    }

    override fun onUnbind(intent: Intent?): Boolean {
        if (bindCount.decrementAndGet() <= 0 && status == SttContract.STATUS_IDLE) {
            stopSelf()
        }
        return false
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int =
        START_NOT_STICKY

    override fun onDestroy() {
        if (recorder?.isRunning() == true) {
            recorder?.stopAndTakeSamples()
        }
        recorder = null
        worker.shutdownNow()
        callbacks.kill()
        super.onDestroy()
    }

    private fun startSession(source: Int) {
        if (status == SttContract.STATUS_RECORDING) {
            stopSession()
            return
        }
        if (status == SttContract.STATUS_TRANSCRIBING) {
            broadcastError("busy")
            return
        }
        activeSource = source
        try {
            ensureNotificationChannel()
            val notification = buildNotification(getString(R.string.notif_recording))
            ServiceCompat.startForeground(
                this,
                NOTIFICATION_ID,
                notification,
                ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE,
            )
            val rec =
                AudioRecorder(
                    onLevel = { level -> broadcastLevel(level) },
                    onMaxDuration = { mainHandler.post { stopSession() } },
                )
            recorder = rec
            rec.start()
            setStatus(SttContract.STATUS_RECORDING)
        } catch (err: Exception) {
            ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
            setStatus(SttContract.STATUS_IDLE)
            broadcastError(err.message ?: "mic failed")
            maybeStopIfUnbound()
        }
    }

    private fun stopSession() {
        if (status != SttContract.STATUS_RECORDING) return
        val rec = recorder
        recorder = null
        setStatus(SttContract.STATUS_TRANSCRIBING)
        updateNotification(getString(R.string.notif_transcribing))
        worker.execute {
            val samples =
                try {
                    rec?.stopAndTakeSamples() ?: FloatArray(0)
                } catch (err: Exception) {
                    mainHandler.post {
                        setStatus(SttContract.STATUS_IDLE)
                        ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
                        broadcastError(err.message ?: "stop failed")
                        maybeStopIfUnbound()
                    }
                    return@execute
                }
            val text =
                try {
                    engine.transcribe(samples, SttContract.SAMPLE_RATE)
                } catch (err: Exception) {
                    mainHandler.post {
                        setStatus(SttContract.STATUS_IDLE)
                        ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
                        broadcastError(err.message ?: "transcribe failed")
                        maybeStopIfUnbound()
                    }
                    return@execute
                }
            mainHandler.post {
                // Always copy for IME so text survives if the keyboard was killed.
                if (activeSource == SttContract.SOURCE_IME || text.isNotEmpty()) {
                    copyToClipboard(text)
                }
                if (text.isNotEmpty() && activeSource == SttContract.SOURCE_IME) {
                    showResultNotification(text)
                }
                broadcastResult(activeSource, text)
                setStatus(SttContract.STATUS_IDLE)
                ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
                maybeStopIfUnbound()
            }
        }
    }

    private fun maybeStopIfUnbound() {
        if (bindCount.get() <= 0 && status == SttContract.STATUS_IDLE) {
            stopSelf()
        }
    }

    private fun setStatus(value: Int) {
        status = value
        broadcastStatus(value)
    }

    private fun broadcastStatus(value: Int) {
        val n = callbacks.beginBroadcast()
        try {
            for (i in 0 until n) {
                try {
                    callbacks.getBroadcastItem(i).onStatus(value)
                } catch (_: RemoteException) {
                }
            }
        } finally {
            callbacks.finishBroadcast()
        }
    }

    private fun broadcastLevel(level: Float) {
        val n = callbacks.beginBroadcast()
        try {
            for (i in 0 until n) {
                try {
                    callbacks.getBroadcastItem(i).onLevel(level)
                } catch (_: RemoteException) {
                }
            }
        } finally {
            callbacks.finishBroadcast()
        }
    }

    private fun broadcastResult(source: Int, text: String) {
        val n = callbacks.beginBroadcast()
        try {
            for (i in 0 until n) {
                try {
                    callbacks.getBroadcastItem(i).onResult(source, text)
                } catch (_: RemoteException) {
                }
            }
        } finally {
            callbacks.finishBroadcast()
        }
    }

    private fun broadcastError(message: String) {
        val n = callbacks.beginBroadcast()
        try {
            for (i in 0 until n) {
                try {
                    callbacks.getBroadcastItem(i).onError(message)
                } catch (_: RemoteException) {
                }
            }
        } finally {
            callbacks.finishBroadcast()
        }
    }

    private fun copyToClipboard(text: String) {
        val clipboard = getSystemService(ClipboardManager::class.java) ?: return
        clipboard.setPrimaryClip(ClipData.newPlainText("dictator", text))
    }

    private fun ensureNotificationChannel() {
        val manager = getSystemService(NotificationManager::class.java) ?: return
        val channel =
            NotificationChannel(
                CHANNEL_ID,
                getString(R.string.notif_channel_name),
                NotificationManager.IMPORTANCE_LOW,
            )
        manager.createNotificationChannel(channel)
    }

    private fun buildNotification(content: String): Notification {
        val launch =
            PendingIntent.getActivity(
                this,
                0,
                Intent(this, SetupActivity::class.java),
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
            )
        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle(getString(R.string.app_name))
            .setContentText(content)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentIntent(launch)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .build()
    }

    private fun updateNotification(content: String) {
        val manager = getSystemService(NotificationManager::class.java) ?: return
        manager.notify(NOTIFICATION_ID, buildNotification(content))
    }

    private fun showResultNotification(text: String) {
        ensureNotificationChannel()
        val manager = getSystemService(NotificationManager::class.java) ?: return
        val preview = if (text.length > 80) text.take(80) + "…" else text
        val notification =
            NotificationCompat.Builder(this, CHANNEL_ID)
                .setContentTitle(getString(R.string.notif_copied_title))
                .setContentText(preview)
                .setSmallIcon(R.drawable.ic_notification)
                .setAutoCancel(true)
                .build()
        manager.notify(RESULT_NOTIFICATION_ID, notification)
    }

    companion object {
        private const val CHANNEL_ID = "dictator"
        private const val NOTIFICATION_ID = 42
        private const val RESULT_NOTIFICATION_ID = 43
    }
}
