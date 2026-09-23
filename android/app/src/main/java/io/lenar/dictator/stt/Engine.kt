package io.lenar.dictator.stt

/**
 * GigaAM runner. Step 2 is stub-only (no sherpa JNI in this process yet beyond the API shape).
 * Real OfflineRecognizer lands in step 3 and must stay inside process `:stt`.
 */
class Engine {
    fun preload() {
        // no-op until sherpa assets land
    }

    fun transcribe(samples: FloatArray, sampleRate: Int): String {
        if (samples.isEmpty() || sampleRate <= 0) return ""
        val duration = samples.size.toDouble() / sampleRate.toDouble()
        if (duration < SttContract.MIN_UTTERANCE_SECONDS) return ""

        // Exercise the same chunking path the real model will use.
        val chunks = Chunker.splitForAsr(samples, sampleRate)
        if (chunks.isEmpty()) return ""
        return "[stub] (${"%.1f".format(duration)}s)"
    }
}
