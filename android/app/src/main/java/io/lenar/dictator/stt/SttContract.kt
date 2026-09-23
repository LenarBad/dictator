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
    const val MAX_SESSION_SECONDS = 180
    const val MIN_UTTERANCE_SECONDS = 0.35
}
