package io.lenar.dictator.stt

import android.content.res.AssetManager
import com.k2fsa.sherpa.onnx.FeatureConfig
import com.k2fsa.sherpa.onnx.OfflineModelConfig
import com.k2fsa.sherpa.onnx.OfflineRecognizer
import com.k2fsa.sherpa.onnx.OfflineRecognizerConfig
import com.k2fsa.sherpa.onnx.OfflineTransducerModelConfig
import java.util.Locale

/**
 * GigaAM-v3 e2e RNNT via sherpa-onnx. Must only be constructed inside process `:stt`
 * so the IME process never [System.loadLibrary]s the JNI.
 */
class Engine(private val assetManager: AssetManager) {
    @Volatile private var recognizer: OfflineRecognizer? = null
    private val stub: Boolean = !hasModelAssets(assetManager)

    fun preload() {
        if (stub) return
        ensureRecognizer()
    }

    fun transcribe(samples: FloatArray, sampleRate: Int): String {
        if (samples.isEmpty() || sampleRate <= 0) return ""
        val duration = samples.size.toDouble() / sampleRate.toDouble()
        if (duration < SttContract.MIN_UTTERANCE_SECONDS) return ""

        if (stub) {
            return "[stub] (${"%.1f".format(Locale.US, duration)}s)"
        }

        val rec = ensureRecognizer()
        val parts = ArrayList<String>()
        for (chunk in Chunker.splitForAsr(samples, sampleRate)) {
            val stream = rec.createStream()
            try {
                stream.acceptWaveform(chunk, sampleRate)
                rec.decode(stream)
                val text = rec.getResult(stream).text.trim()
                if (text.isNotEmpty()) {
                    parts.add(text)
                }
            } finally {
                stream.release()
            }
        }
        return parts.joinToString(" ").trim()
    }

    @Synchronized
    private fun ensureRecognizer(): OfflineRecognizer {
        recognizer?.let {
            return it
        }
        val config =
            OfflineRecognizerConfig(
                featConfig =
                    FeatureConfig(
                        sampleRate = SttContract.SAMPLE_RATE,
                        featureDim = FEATURE_DIM,
                    ),
                modelConfig =
                    OfflineModelConfig(
                        transducer =
                            OfflineTransducerModelConfig(
                                encoder = "gigaam/encoder.int8.onnx",
                                decoder = "gigaam/decoder.onnx",
                                joiner = "gigaam/joiner.onnx",
                            ),
                        tokens = "gigaam/tokens.txt",
                        numThreads = NUM_THREADS,
                        provider = "cpu",
                        modelType = "nemo_transducer",
                    ),
                decodingMethod = "greedy_search",
            )
        val created = OfflineRecognizer(assetManager, config)
        recognizer = created
        return created
    }

    companion object {
        private const val FEATURE_DIM = 64
        private const val NUM_THREADS = 2

        private val MODEL_FILES =
            listOf(
                "gigaam/encoder.int8.onnx",
                "gigaam/decoder.onnx",
                "gigaam/joiner.onnx",
                "gigaam/tokens.txt",
            )

        fun hasModelAssets(assetManager: AssetManager): Boolean {
            return MODEL_FILES.all { path ->
                try {
                    assetManager.open(path).close()
                    true
                } catch (_: Exception) {
                    false
                }
            }
        }
    }
}
