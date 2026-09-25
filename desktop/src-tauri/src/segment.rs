//! When to hand the open microphone buffer to recognition.
//!
//! Same cuts as Android `SegmentGate`: rotate on a pause after three minutes,
//! force a cut thirty seconds later, stop a forgotten session at one hour.
//! Speaker diarization never rotates — the whole session is one pass at the end.

pub const SEGMENT_SECONDS: f64 = 180.0;
pub const SEGMENT_FORCE_EXTRA_SECONDS: f64 = 30.0;
pub const SILENCE_SECONDS: f64 = 0.7;
pub const SILENCE_RMS: f64 = 0.02;
pub const MAX_SESSION_SECONDS: f64 = 60.0 * 60.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Continue,
    Rotate,
    Stop,
}

/// `session_seconds` is audio already handed off. `segment_seconds` is the open buffer.
/// Stop wins over rotate: the open buffer is the tail of the session.
pub fn action(
    segment_seconds: f64,
    session_seconds: f64,
    silence_seconds: f64,
    diarize: bool,
) -> Action {
    if !segment_seconds.is_finite()
        || segment_seconds < 0.0
        || !session_seconds.is_finite()
        || session_seconds < 0.0
    {
        return Action::Continue;
    }
    if session_seconds + segment_seconds >= MAX_SESSION_SECONDS {
        return Action::Stop;
    }
    if diarize {
        return Action::Continue;
    }
    if segment_seconds >= SEGMENT_SECONDS + SEGMENT_FORCE_EXTRA_SECONDS {
        return Action::Rotate;
    }
    let silence = if silence_seconds.is_finite() {
        silence_seconds
    } else {
        0.0
    };
    if segment_seconds >= SEGMENT_SECONDS && silence >= SILENCE_SECONDS {
        return Action::Rotate;
    }
    Action::Continue
}

/// Trailing quiet time after this chunk. Silence counts only once the open
/// buffer has reached [SEGMENT_SECONDS], and a loud chunk clears it.
pub fn next_silence(
    segment_seconds: f64,
    chunk_seconds: f64,
    chunk_rms: f64,
    previous_silence: f64,
) -> f64 {
    if !segment_seconds.is_finite()
        || segment_seconds < SEGMENT_SECONDS
        || !chunk_rms.is_finite()
        || chunk_rms >= SILENCE_RMS
    {
        return 0.0;
    }
    let chunk = if chunk_seconds.is_finite() && chunk_seconds > 0.0 {
        chunk_seconds
    } else {
        0.0
    };
    let previous = if previous_silence.is_finite() && previous_silence > 0.0 {
        previous_silence
    } else {
        0.0
    };
    previous + chunk
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_recording_before_the_segment_mark() {
        assert_eq!(
            action(SEGMENT_SECONDS - 0.1, 0.0, 5.0, false),
            Action::Continue
        );
    }

    #[test]
    fn rotates_on_pause_after_three_minutes() {
        assert_eq!(
            action(SEGMENT_SECONDS, 0.0, SILENCE_SECONDS - 0.01, false),
            Action::Continue
        );
        assert_eq!(
            action(SEGMENT_SECONDS, 0.0, SILENCE_SECONDS, false),
            Action::Rotate
        );
    }

    #[test]
    fn cuts_when_no_pause_arrives() {
        let force = SEGMENT_SECONDS + SEGMENT_FORCE_EXTRA_SECONDS;
        assert_eq!(action(force, 0.0, 0.0, false), Action::Rotate);
    }

    #[test]
    fn session_cap_stops_instead_of_rotating() {
        let session = MAX_SESSION_SECONDS - 10.0;
        assert_eq!(action(10.0, session, 0.0, false), Action::Stop);
        assert_eq!(action(10.0, session, 0.0, true), Action::Stop);
    }

    #[test]
    fn diarization_holds_the_buffer_until_the_hour() {
        let force = SEGMENT_SECONDS + SEGMENT_FORCE_EXTRA_SECONDS;
        assert_eq!(action(force, 0.0, SILENCE_SECONDS, true), Action::Continue);
        assert_eq!(
            action(1.0, MAX_SESSION_SECONDS - 1.0, 0.0, true),
            Action::Stop
        );
    }

    #[test]
    fn silence_starts_only_after_three_minutes_and_resets_when_loud() {
        assert_eq!(next_silence(SEGMENT_SECONDS - 0.1, 0.2, 0.0, 1.0), 0.0);
        assert_eq!(next_silence(SEGMENT_SECONDS, 0.2, SILENCE_RMS, 1.0), 0.0);
        assert_eq!(
            next_silence(SEGMENT_SECONDS, 0.2, SILENCE_RMS - 0.001, 0.5),
            0.7
        );
    }
}
