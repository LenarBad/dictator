package io.lenar.dictator.stt

/**
 * When to hand the current buffer to recognition without stopping the microphone.
 * [Action.STOP] wins over [Action.ROTATE]: the open buffer is the tail of the session.
 */
object SegmentGate {
    enum class Action { CONTINUE, ROTATE, STOP }

    fun action(
        segmentSamples: Int,
        sessionSamples: Int,
        silentSamples: Int,
        sampleRate: Int,
    ): Action {
        if (sampleRate <= 0 || segmentSamples < 0 || sessionSamples < 0) return Action.CONTINUE
        val sessionLimit = sampleRate.toLong() * SttContract.MAX_SESSION_SECONDS
        val total = sessionSamples.toLong() + segmentSamples.toLong()
        if (total >= sessionLimit) return Action.STOP

        val segmentLimit = sampleRate * SttContract.SEGMENT_SECONDS
        val forceLimit =
            sampleRate * (SttContract.SEGMENT_SECONDS + SttContract.SEGMENT_FORCE_EXTRA_SECONDS)
        val silenceNeeded = (SttContract.SILENCE_SECONDS * sampleRate).toInt()
        if (segmentSamples >= forceLimit) return Action.ROTATE
        if (segmentSamples >= segmentLimit && silentSamples >= silenceNeeded) return Action.ROTATE
        return Action.CONTINUE
    }
}
