package io.lenar.dictator.stt

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.RemoteCallbackList
import android.os.RemoteException
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import io.lenar.dictator.R
import io.lenar.dictator.settings.SetupActivity
import java.util.concurrent.ConcurrentLinkedQueue
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean
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
    @Volatile private var sessionId: Int = 0
    @Volatile private var liveGeneration: Int = 0
    @Volatile private var segmentEpoch: Int = 0
    @Volatile private var liveChunksEnabled: Boolean = false
    private var recorder: AudioRecorder? = null
    private val transcriptLock = Any()
    private val confirmedParts = ArrayList<String>()
    private val heldParts = ArrayDeque<String>()
    private val liveParts = ArrayList<String>()
    private var liveNextWindow = 0
    private val pendingSegments = ConcurrentLinkedQueue<PendingSegment>()
    private val drainScheduled = AtomicBoolean(false)

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

            override fun start(source: Int, liveChunks: Boolean) {
                mainHandler.post { startSession(source, liveChunks) }
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

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        // startForegroundService shows the system "Starting FGS…" placeholder until this
        // returns. Do it before any other work, including the tile's later AIDL start().
        if (!promoteToForeground(getString(R.string.notif_recording))) {
            return START_NOT_STICKY
        }
        if (intent?.action == ACTION_STOP) {
            mainHandler.post { stopSession() }
        }
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        if (recorder?.isRunning() == true) {
            recorder?.stopAndTakeSamples()
        }
        recorder = null
        worker.shutdownNow()
        callbacks.kill()
        super.onDestroy()
    }

    private fun startSession(source: Int, liveChunks: Boolean) {
        if (status == SttContract.STATUS_RECORDING) {
            stopSession()
            return
        }
        if (status == SttContract.STATUS_TRANSCRIBING) {
            broadcastError("busy")
            return
        }
        activeSource = source
        val session =
            synchronized(transcriptLock) {
                sessionId++
                liveGeneration++
                segmentEpoch++
                confirmedParts.clear()
                heldParts.clear()
                liveParts.clear()
                liveNextWindow = 0
                sessionId
            }
        pendingSegments.clear()
        liveChunksEnabled = liveChunks && source == SttContract.SOURCE_TILE
        val chunkListener =
            if (liveChunksEnabled) {
                { snapshot: FloatArray, readyCount: Int ->
                    val epoch = synchronized(transcriptLock) { segmentEpoch }
                    mainHandler.post {
                        if (epoch != segmentEpoch || session != sessionId ||
                            status != SttContract.STATUS_RECORDING
                        ) {
                            return@post
                        }
                        scheduleLiveWindows(snapshot, readyCount, epoch)
                    }
                    Unit
                }
            } else {
                null
            }
        try {
            if (!promoteToForeground(getString(R.string.notif_recording))) return
            val rec =
                AudioRecorder(
                    onLevel = { level -> broadcastLevel(level) },
                    onSessionLimit = { mainHandler.post { stopSession() } },
                    onSegment = { samples -> handoffSegment(session, samples) },
                    onCompleteWindows = chunkListener,
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

    private fun handoffSegment(session: Int, samples: FloatArray) {
        if (samples.isEmpty() || session != sessionId) return
        synchronized(transcriptLock) {
            if (session != sessionId) return
            heldParts.addLast(liveParts.joinToString(" "))
            liveParts.clear()
            liveNextWindow = 0
            liveGeneration++
            segmentEpoch++
        }
        pendingSegments.add(PendingSegment(session, samples))
        scheduleDrain()
    }

    private fun scheduleDrain() {
        if (!drainScheduled.compareAndSet(false, true)) return
        worker.execute {
            try {
                transcribeQueued()
            } finally {
                drainScheduled.set(false)
                if (pendingSegments.isNotEmpty()) scheduleDrain()
            }
        }
    }

    private fun transcribeQueued() {
        while (true) {
            val next = pendingSegments.poll() ?: return
            if (next.session != sessionId) continue
            val text =
                try {
                    engine.transcribe(next.samples, SttContract.SAMPLE_RATE)
                } catch (err: Exception) {
                    mainHandler.post { abortSession(next.session, err.message ?: "transcribe failed") }
                    return
                }
            val preview =
                synchronized(transcriptLock) {
                    if (next.session != sessionId) return
                    if (heldParts.isNotEmpty()) heldParts.removeFirst()
                    if (text.isNotEmpty()) confirmedParts.add(text)
                    previewLocked()
                }
            publishPreview(next.session, preview)
        }
    }

    private fun scheduleLiveWindows(snapshot: FloatArray, readyCount: Int, epoch: Int) {
        val from: Int
        val generation: Int
        synchronized(transcriptLock) {
            if (epoch != segmentEpoch) return
            from = liveNextWindow
            if (from >= readyCount) return
            liveNextWindow = readyCount
            generation = liveGeneration
        }
        worker.execute {
            for (index in from until readyCount) {
                if (generation != liveGeneration || epoch != segmentEpoch) return@execute
                val text =
                    try {
                        engine.transcribe(
                            Chunker.windowAt(snapshot, SttContract.SAMPLE_RATE, index),
                            SttContract.SAMPLE_RATE,
                        )
                    } catch (_: Exception) {
                        ""
                    }
                val preview =
                    synchronized(transcriptLock) {
                        if (generation != liveGeneration || epoch != segmentEpoch) return@execute
                        if (text.isNotEmpty()) liveParts.add(text)
                        previewLocked()
                    }
                publishPreview(sessionId, preview)
            }
        }
    }

    private fun publishPreview(session: Int, preview: String) {
        if (!liveChunksEnabled) return
        mainHandler.post {
            if (session == sessionId && status == SttContract.STATUS_RECORDING) {
                broadcastPartial(activeSource, preview)
            }
        }
    }

    private fun previewLocked(): String {
        val parts = ArrayList<String>(confirmedParts.size + heldParts.size + liveParts.size)
        confirmedParts.filterTo(parts) { it.isNotEmpty() }
        heldParts.filterTo(parts) { it.isNotEmpty() }
        liveParts.filterTo(parts) { it.isNotEmpty() }
        return parts.joinToString(" ")
    }

    private fun stopSession() {
        if (status != SttContract.STATUS_RECORDING) return
        val session = sessionId
        synchronized(transcriptLock) {
            liveGeneration++
            segmentEpoch++
            liveParts.clear()
            liveNextWindow = 0
        }
        val rec = recorder
        recorder = null
        setStatus(SttContract.STATUS_TRANSCRIBING)
        updateNotification(getString(R.string.notif_transcribing), stopAction = false)
        worker.execute {
            val tail =
                try {
                    rec?.stopAndTakeSamples() ?: FloatArray(0)
                } catch (err: Exception) {
                    mainHandler.post { abortSession(session, err.message ?: "stop failed") }
                    return@execute
                }
            if (tail.isNotEmpty()) pendingSegments.add(PendingSegment(session, tail))
            try {
                transcribeQueued()
            } catch (_: Exception) {
                return@execute
            }
            val text =
                synchronized(transcriptLock) {
                    if (session != sessionId) return@execute
                    confirmedParts.joinToString(" ")
                }
            mainHandler.post {
                if (session != sessionId) return@post
                deliverBySource(activeSource, text)
                broadcastResult(activeSource, text)
                setStatus(SttContract.STATUS_IDLE)
                ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
                maybeStopIfUnbound()
            }
        }
    }

    private fun abortSession(session: Int, message: String) {
        if (session != sessionId) return
        sessionId++
        val rec = recorder
        recorder = null
        pendingSegments.clear()
        try {
            rec?.stopAndTakeSamples()
        } catch (_: Exception) {
        }
        setStatus(SttContract.STATUS_IDLE)
        ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
        broadcastError(message)
        maybeStopIfUnbound()
    }

    private fun deliverBySource(source: Int, text: String) {
        when (source) {
            SttContract.SOURCE_IME -> {
                copyToClipboard(text)
                if (text.isNotEmpty()) {
                    showResultNotification(text, getString(R.string.notif_copied_title))
                }
            }
            SttContract.SOURCE_TILE -> {
                copyToClipboard(text)
                showResultNotification(
                    if (text.isEmpty()) getString(R.string.notif_empty_body) else text,
                    getString(R.string.notif_copied_paste_title),
                )
            }
            SttContract.SOURCE_RECOG -> {
                // Prefer Bundle delivery via RecognitionService; clipboard is fallback.
                if (text.isNotEmpty()) {
                    copyToClipboard(text)
                }
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

    private fun broadcastPartial(source: Int, text: String) {
        val n = callbacks.beginBroadcast()
        try {
            for (i in 0 until n) {
                try {
                    callbacks.getBroadcastItem(i).onPartial(source, text)
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

    /** @return false when Android rejected the microphone FGS (app not in the foreground). */
    private fun promoteToForeground(content: String): Boolean {
        return try {
            ensureNotificationChannel()
            ServiceCompat.startForeground(
                this,
                NOTIFICATION_ID,
                buildNotification(content, stopAction = true),
                ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE,
            )
            true
        } catch (err: Exception) {
            setStatus(SttContract.STATUS_IDLE)
            broadcastError(err.message ?: "mic failed")
            stopSelf()
            false
        }
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

    private fun buildNotification(content: String, stopAction: Boolean): Notification {
        val launch =
            PendingIntent.getActivity(
                this,
                0,
                Intent(this, SetupActivity::class.java),
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
            )
        val builder =
            NotificationCompat.Builder(this, CHANNEL_ID)
                .setContentTitle(getString(R.string.app_name))
                .setContentText(content)
                .setSmallIcon(R.drawable.ic_notification)
                .setContentIntent(launch)
                .setOngoing(true)
                .setOnlyAlertOnce(true)
        if (stopAction) {
            val stop =
                PendingIntent.getForegroundService(
                    this,
                    1,
                    Intent(this, SttService::class.java).setAction(ACTION_STOP),
                    PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
                )
            builder.addAction(0, getString(R.string.tile_session_stop), stop)
        }
        return builder.build()
    }

    private fun updateNotification(content: String, stopAction: Boolean) {
        val manager = getSystemService(NotificationManager::class.java) ?: return
        manager.notify(NOTIFICATION_ID, buildNotification(content, stopAction))
    }

    private fun showResultNotification(text: String, title: String) {
        ensureNotificationChannel()
        val manager = getSystemService(NotificationManager::class.java) ?: return
        val preview = if (text.length > 80) text.take(80) + "…" else text
        val notification =
            NotificationCompat.Builder(this, CHANNEL_ID)
                .setContentTitle(title)
                .setContentText(preview)
                .setSmallIcon(R.drawable.ic_notification)
                .setAutoCancel(true)
                .build()
        manager.notify(RESULT_NOTIFICATION_ID, notification)
    }

    private data class PendingSegment(val session: Int, val samples: FloatArray)

    companion object {
        private const val CHANNEL_ID = "dictator"
        private const val NOTIFICATION_ID = 42
        private const val RESULT_NOTIFICATION_ID = 43
        private const val ACTION_STOP = "io.lenar.dictator.stt.STOP"

        /** Marks [:stt] started so a tile session survives dialog dismiss. */
        fun ensureStarted(context: Context) {
            ContextCompat.startForegroundService(context, Intent(context, SttService::class.java))
        }
    }
}
