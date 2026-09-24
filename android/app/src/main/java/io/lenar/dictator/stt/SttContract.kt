package io.lenar.dictator.stt

/** AIDL source / status constants shared by IME and :stt. */
object SttContract {
    const val SOURCE_IME = 1
    const val SOURCE_TILE = 2
    const val SOURCE_RECOG = 3

    const val STATUS_IDLE = 0
    const val STATUS_RECORDING = 1
    const val STATUS_TRANSCRIBING = 2

    const val SAMPLE_RATE = 16_000

    /** Hand the buffer to recognition after this long, on a pause when one arrives. */
    const val SEGMENT_SECONDS = 180

    /** Cut the segment even mid-phrase when no pause arrives. */
    const val SEGMENT_FORCE_EXTRA_SECONDS = 30

    /** Quiet stretch that counts as a pause once [SEGMENT_SECONDS] is reached. */
    const val SILENCE_SECONDS = 0.7
    const val SILENCE_RMS = 0.02f

    /** Forgotten session stops here. Speech before that is kept as text. */
    const val MAX_SESSION_SECONDS = 60 * 60

    const val MIN_UTTERANCE_SECONDS = 0.35
}
