package io.lenar.dictator.stt

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ChunkerTest {
    @Test
    fun shortAudioIsSingleChunk() {
        val rate = 16_000
        val samples = FloatArray(rate * 10) { 0.1f }
        val chunks = Chunker.splitForAsr(samples, rate)
        assertEquals(1, chunks.size)
        assertEquals(samples.size, chunks[0].size)
    }

    @Test
    fun longAudioIsSplitWithOverlap() {
        val rate = 16_000
        val samples = FloatArray(rate * 50) { 0.1f }
        val chunks = Chunker.splitForAsr(samples, rate)
        assertTrue(chunks.size >= 3)
        assertEquals(rate * 20, chunks[0].size)
    }

    @Test
    fun emptyIsEmpty() {
        assertTrue(Chunker.splitForAsr(FloatArray(0), 16_000).isEmpty())
    }

    @Test
    fun completeWindowsMatchFullChunks() {
        val rate = 16_000
        val samples = FloatArray(rate * 50) { 0.1f }
        val ready = Chunker.completeWindowCount(samples.size, rate)
        val split = Chunker.splitForAsr(samples, rate)
        assertEquals(2, ready)
        assertTrue(split.size > ready)
        for (index in 0 until ready) {
            assertEquals(rate * 20, Chunker.windowAt(samples, rate, index).size)
        }
    }

    @Test
    fun shortAudioHasNoCompleteWindow() {
        val rate = 16_000
        assertEquals(0, Chunker.completeWindowCount(rate * 10, rate))
    }
}
