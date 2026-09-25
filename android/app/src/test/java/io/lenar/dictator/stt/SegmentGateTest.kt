package io.lenar.dictator.stt

import org.junit.Assert.assertEquals
import org.junit.Test

class SegmentGateTest {
    private val rate = 16_000

    @Test
    fun keepsRecordingBeforeTheSegmentMark() {
        val almost = rate * SttContract.SEGMENT_SECONDS - 1
        assertEquals(
            SegmentGate.Action.CONTINUE,
            SegmentGate.action(almost, 0, rate * 5, rate),
        )
    }

    @Test
    fun rotatesOnPauseAfterThreeMinutes() {
        val segment = rate * SttContract.SEGMENT_SECONDS
        val silence = (SttContract.SILENCE_SECONDS * rate).toInt()
        assertEquals(
            SegmentGate.Action.CONTINUE,
            SegmentGate.action(segment, 0, silence - 1, rate),
        )
        assertEquals(
            SegmentGate.Action.ROTATE,
            SegmentGate.action(segment, 0, silence, rate),
        )
    }

    @Test
    fun cutsWhenNoPauseArrives() {
        val force =
            rate * (SttContract.SEGMENT_SECONDS + SttContract.SEGMENT_FORCE_EXTRA_SECONDS)
        assertEquals(SegmentGate.Action.ROTATE, SegmentGate.action(force, 0, 0, rate))
    }

    @Test
    fun sessionCapStopsInsteadOfRotating() {
        val session = rate * (SttContract.MAX_SESSION_SECONDS - 10)
        val segment = rate * 10
        assertEquals(SegmentGate.Action.STOP, SegmentGate.action(segment, session, 0, rate))
    }
}
