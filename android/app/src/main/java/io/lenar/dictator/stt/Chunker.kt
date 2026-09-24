package io.lenar.dictator.stt

/**
 * Split long PCM the same way as desktop [wav::split_for_asr]:
 * under ~24 s → one chunk; longer → 20 s windows with 0.4 s overlap.
 */
object Chunker {
    private const val MAX_SHORT_SECONDS = 24.0
    private const val CHUNK_SECONDS = 20.0
    private const val OVERLAP_SECONDS = 0.4
    private const val MIN_CHUNK_SECONDS = 0.2

    fun splitForAsr(samples: FloatArray, sampleRate: Int): List<FloatArray> {
        if (samples.isEmpty() || sampleRate <= 0) return emptyList()
        val maxShort = (MAX_SHORT_SECONDS * sampleRate).toInt()
        if (samples.size <= maxShort) return listOf(samples.copyOf())

        val chunk = (CHUNK_SECONDS * sampleRate).toInt()
        val overlap = (OVERLAP_SECONDS * sampleRate).toInt()
        val hop = (chunk - overlap).coerceAtLeast(1)
        val minChunk = (MIN_CHUNK_SECONDS * sampleRate).toInt()
        val out = ArrayList<FloatArray>()
        var start = 0
        while (start < samples.size) {
            val end = (start + chunk).coerceAtMost(samples.size)
            if (end - start < minChunk && out.isNotEmpty()) break
            out.add(samples.copyOfRange(start, end))
            if (end >= samples.size) break
            start += hop
        }
        return out
    }

    /** Full 20 s windows already captured. The short tail waits for the final pass. */
    fun completeWindowCount(sampleCount: Int, sampleRate: Int): Int {
        if (sampleCount <= 0 || sampleRate <= 0) return 0
        val chunk = chunkSamples(sampleRate)
        if (sampleCount < chunk) return 0
        val hop = hopSamples(sampleRate)
        return ((sampleCount - chunk) / hop) + 1
    }

    /** Window [index] using the same hop as [splitForAsr]. */
    fun windowAt(samples: FloatArray, sampleRate: Int, index: Int): FloatArray {
        val chunk = chunkSamples(sampleRate)
        val start = index * hopSamples(sampleRate)
        if (start >= samples.size) return FloatArray(0)
        val end = (start + chunk).coerceAtMost(samples.size)
        return samples.copyOfRange(start, end)
    }

    private fun chunkSamples(sampleRate: Int) = (CHUNK_SECONDS * sampleRate).toInt()

    private fun hopSamples(sampleRate: Int): Int {
        val overlap = (OVERLAP_SECONDS * sampleRate).toInt()
        return (chunkSamples(sampleRate) - overlap).coerceAtLeast(1)
    }
}
