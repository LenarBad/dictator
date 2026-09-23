//! Turn labels for optional diarization. No ONNX.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub start: f64,
    pub end: f64,
    pub speaker: i32,
}

/// Collapse consecutive segments of the same speaker into one time range.
/// Result is ordered by start time. Empty intervals are dropped.
pub fn merge_adjacent(segments: &[Segment]) -> Vec<Segment> {
    let mut ordered: Vec<Segment> = segments
        .iter()
        .copied()
        .filter(|segment| {
            segment.start.is_finite() && segment.end.is_finite() && segment.end > segment.start
        })
        .collect();
    ordered.sort_by(|left, right| {
        left.start
            .partial_cmp(&right.start)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut merged: Vec<Segment> = Vec::new();
    for segment in ordered {
        if let Some(last) = merged.last_mut() {
            if last.speaker == segment.speaker {
                if segment.end > last.end {
                    last.end = segment.end;
                }
                continue;
            }
        }
        merged.push(segment);
    }
    merged
}

/// `texts` lines up with `segments` by index.
/// One distinct speaker (after dropping empty text) is a plain line.
/// Two or more are `Спикер N` in order of first appearance, with a blank line between turns.
pub fn format(segments: &[Segment], texts: &[String]) -> String {
    let mut turns: Vec<(i32, String)> = Vec::new();
    let mut previous_speaker: Option<i32> = None;
    for (index, segment) in segments.iter().enumerate() {
        let text = texts.get(index).map(|value| value.trim()).unwrap_or("");
        let same_speaker = previous_speaker == Some(segment.speaker);
        previous_speaker = Some(segment.speaker);
        if text.is_empty() {
            continue;
        }
        if same_speaker {
            if let Some((speaker, existing)) = turns.last_mut() {
                if *speaker == segment.speaker {
                    if !existing.is_empty() {
                        existing.push(' ');
                    }
                    existing.push_str(text);
                    continue;
                }
            }
        }
        turns.push((segment.speaker, text.to_string()));
    }

    let mut order = Vec::new();
    for (speaker, _) in &turns {
        if !order.contains(speaker) {
            order.push(*speaker);
        }
    }
    if order.len() <= 1 {
        return turns
            .into_iter()
            .map(|(_, text)| text)
            .collect::<Vec<_>>()
            .join(" ");
    }

    turns
        .into_iter()
        .map(|(speaker, text)| {
            let number = order
                .iter()
                .position(|id| *id == speaker)
                .map(|index| index + 1)
                .unwrap_or(1);
            format!("Спикер {number}: {text}")
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start: f64, end: f64, speaker: i32) -> Segment {
        Segment {
            start,
            end,
            speaker,
        }
    }

    #[test]
    fn no_segments_is_empty() {
        assert_eq!(format(&[], &[]), "");
    }

    #[test]
    fn one_speaker_has_no_prefix() {
        let segments = vec![seg(0.0, 1.0, 3)];
        let text = format(&segments, &["одна фраза".to_string()]);
        assert_eq!(text, "одна фраза");
        assert!(!text.contains("Спикер"));
    }

    #[test]
    fn two_speakers_are_labeled() {
        let segments = vec![seg(0.0, 1.0, 0), seg(1.2, 2.0, 1)];
        let texts = vec![
            "Добрый день, давайте начнём.".to_string(),
            "Хорошо, я готов.".to_string(),
        ];
        assert_eq!(
            format(&segments, &texts),
            "Спикер 1: Добрый день, давайте начнём.\n\nСпикер 2: Хорошо, я готов."
        );
    }

    #[test]
    fn adjacent_same_speaker_is_merged() {
        let segments = vec![seg(0.0, 1.0, 0), seg(1.1, 2.0, 0), seg(2.2, 3.0, 1)];
        let texts = vec![
            "привет".to_string(),
            "как дела".to_string(),
            "хорошо".to_string(),
        ];
        assert_eq!(
            format(&segments, &texts),
            "Спикер 1: привет как дела\n\nСпикер 2: хорошо"
        );
    }

    #[test]
    fn merge_adjacent_extends_the_time_range() {
        let segments = vec![seg(0.0, 1.0, 0), seg(1.4, 2.0, 0), seg(2.2, 3.0, 1)];
        let merged = merge_adjacent(&segments);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0], seg(0.0, 2.0, 0));
        assert_eq!(merged[1], seg(2.2, 3.0, 1));
    }

    #[test]
    fn empty_segment_text_is_omitted() {
        let segments = vec![seg(0.0, 1.0, 0), seg(1.0, 2.0, 1), seg(2.0, 3.0, 1)];
        let texts = vec!["привет".to_string(), "   ".to_string(), "пока".to_string()];
        assert_eq!(
            format(&segments, &texts),
            "Спикер 1: привет\n\nСпикер 2: пока"
        );
    }

    #[test]
    fn numbers_follow_first_appearance() {
        let segments = vec![seg(0.0, 1.0, 5), seg(1.0, 2.0, 2), seg(2.0, 3.0, 5)];
        let texts = vec!["а".to_string(), "б".to_string(), "в".to_string()];
        assert_eq!(
            format(&segments, &texts),
            "Спикер 1: а\n\nСпикер 2: б\n\nСпикер 1: в"
        );
    }
}
