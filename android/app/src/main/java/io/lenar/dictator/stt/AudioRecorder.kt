package io.lenar.dictator.stt

import android.annotation.SuppressLint
import android.media.AudioFormat
import android.media.AudioRecord
import android.media.MediaRecorder
import android.os.Process
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.math.sqrt

/**
 * 16 kHz mono PCM16 capture into float [-1, 1]. No WAV on disk.
 *
 * The microphone stays open. After [SttContract.SEGMENT_SECONDS] the current buffer is
 * handed to [onSegment] on a pause, or forced a little later. The session itself stops
 * only at [SttContract.MAX_SESSION_SECONDS] via [onSessionLimit].
 */
class AudioRecorder(
    private val onLevel: (Float) -> Unit,
    private val onSessionLimit: () -> Unit,
    private val onSegment: (FloatArray) -> Unit,
    private val onCompleteWindows: ((snapshot: FloatArray, readyCount: Int) -> Unit)? = null,
) {
    private val running = AtomicBoolean(false)
    private var thread: Thread? = null
    private val samples = ArrayList<Float>(SttContract.SAMPLE_RATE * 8)
    private var announcedWindows = 0
    private var sessionSamples = 0
    private var silentSamples = 0

    @SuppressLint("MissingPermission")
    fun start() {
        if (!running.compareAndSet(false, true)) return
        samples.clear()
        val sampleRate = SttContract.SAMPLE_RATE
        val minBuf =
            AudioRecord.getMinBufferSize(
                sampleRate,
                AudioFormat.CHANNEL_IN_MONO,
                AudioFormat.ENCODING_PCM_16BIT,
            )
        if (minBuf <= 0) {
            running.set(false)
            throw IllegalStateException("AudioRecord buffer unavailable")
        }
        val recorder =
            AudioRecord(
                MediaRecorder.AudioSource.VOICE_RECOGNITION,
                sampleRate,
                AudioFormat.CHANNEL_IN_MONO,
                AudioFormat.ENCODING_PCM_16BIT,
                minBuf * 2,
            )
        if (recorder.state != AudioRecord.STATE_INITIALIZED) {
            recorder.release()
            running.set(false)
            throw IllegalStateException("AudioRecord init failed")
        }

        val segmentLimit = sampleRate * SttContract.SEGMENT_SECONDS
        thread =
            Thread({
                Process.setThreadPriority(Process.THREAD_PRIORITY_AUDIO)
                val shortBuf = ShortArray(minBuf)
                recorder.startRecording()
                try {
                    while (running.get()) {
                        val read = recorder.read(shortBuf, 0, shortBuf.size)
                        if (read <= 0) continue
                        var sumSq = 0.0
                        synchronized(samples) {
                            for (i in 0 until read) {
                                val f = shortBuf[i] / 32768.0f
                                samples.add(f)
                                sumSq += (f * f).toDouble()
                            }
                        }
                        val rms = sqrt(sumSq / read).toFloat().coerceIn(0f, 1f)
                        onLevel(rms)
                        val size =
                            synchronized(samples) {
                                samples.size
                            }
                        notifyCompleteWindows(size)
                        if (size < segmentLimit || rms >= SttContract.SILENCE_RMS) {
                            silentSamples = 0
                        } else {
                            silentSamples += read
                        }
                        when (SegmentGate.action(size, sessionSamples, silentSamples, sampleRate)) {
                            SegmentGate.Action.STOP -> {
                                running.set(false)
                                onSessionLimit()
                                break
                            }
                            SegmentGate.Action.ROTATE -> {
                                val taken = takeSegment()
                                sessionSamples += taken.size
                                silentSamples = 0
                                if (taken.isNotEmpty()) onSegment(taken)
                            }
                            SegmentGate.Action.CONTINUE -> Unit
                        }
                    }
                } finally {
                    try {
                        recorder.stop()
                    } catch (_: IllegalStateException) {
                    }
                    recorder.release()
                }
            }, "dictator-audio")
        thread?.start()
    }

    fun stopAndTakeSamples(): FloatArray {
        running.set(false)
        thread?.join(2_000)
        thread = null
        return synchronized(samples) {
            val out = samples.toFloatArray()
            samples.clear()
            out
        }
    }

    fun isRunning(): Boolean = running.get()

    private fun takeSegment(): FloatArray {
        return synchronized(samples) {
            val out = samples.toFloatArray()
            samples.clear()
            announcedWindows = 0
            out
        }
    }

    private fun notifyCompleteWindows(size: Int) {
        val listener = onCompleteWindows ?: return
        val ready = Chunker.completeWindowCount(size, SttContract.SAMPLE_RATE)
        if (ready <= announcedWindows) return
        announcedWindows = ready
        val snapshot =
            synchronized(samples) {
                samples.toFloatArray()
            }
        listener(snapshot, ready)
    }
}
