package io.lenar.dictator.tile

import io.lenar.dictator.stt.SttContract

/** What a tile window should do when it appears, including after a recreate. */
internal enum class TileSessionLaunch {
    /** No session yet: open the mic. */
    Start,

    /** Mic or transcription is already running: keep it, do not call start(). */
    Attach,

    /** Finished text is on screen: stay there, do not open the mic again. */
    RestoreReview,
}

internal fun tileSessionLaunch(status: Int, review: Boolean): TileSessionLaunch {
    return when (status) {
        SttContract.STATUS_RECORDING, SttContract.STATUS_TRANSCRIBING -> TileSessionLaunch.Attach
        else -> if (review) TileSessionLaunch.RestoreReview else TileSessionLaunch.Start
    }
}
