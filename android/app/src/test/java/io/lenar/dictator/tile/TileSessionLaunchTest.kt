package io.lenar.dictator.tile

import io.lenar.dictator.stt.SttContract
import org.junit.Assert.assertEquals
import org.junit.Test

class TileSessionLaunchTest {
    @Test
    fun freshOpenStartsRecording() {
        assertEquals(
            TileSessionLaunch.Start,
            tileSessionLaunch(SttContract.STATUS_IDLE, review = false),
        )
    }

    @Test
    fun rotationDuringRecordingDoesNotRestart() {
        assertEquals(
            TileSessionLaunch.Attach,
            tileSessionLaunch(SttContract.STATUS_RECORDING, review = false),
        )
    }

    @Test
    fun rotationDuringTranscriptionDoesNotRestart() {
        assertEquals(
            TileSessionLaunch.Attach,
            tileSessionLaunch(SttContract.STATUS_TRANSCRIBING, review = false),
        )
    }

    @Test
    fun rotationOnTheDoneScreenDoesNotStartAgain() {
        assertEquals(
            TileSessionLaunch.RestoreReview,
            tileSessionLaunch(SttContract.STATUS_IDLE, review = true),
        )
    }
}
