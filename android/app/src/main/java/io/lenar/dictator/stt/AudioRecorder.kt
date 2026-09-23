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
 * Caps at [SttContract.MAX_SESSION_SECONDS]; caller should also stop on that signal.
 */
class AudioRecorder(
    private val onLevel: (Float) -> Unit,
    private val onMaxDuration: () -> Unit,
) {
    private val running = AtomicBoolean(false)
    private var thread: Thread? = null
    private val samples = ArrayList<Float>(SttContract.SAMPLE_RATE * 8)

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

        val maxSamples = sampleRate * SttContract.MAX_SESSION_SECONDS
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
                        onLevel(sqrt(sumSq / read).toFloat().coerceIn(0f, 1f))
                        val size =
                            synchronized(samples) {
                                samples.size
                            }
                        if (size >= maxSamples) {
                            running.set(false)
                            onMaxDuration()
                            break
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
}
