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
}
