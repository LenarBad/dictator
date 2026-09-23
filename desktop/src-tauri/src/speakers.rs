//! Turn labels for optional diarization. No ONNX.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub start: f64,
    pub end: f64,
    pub speaker: i32,
}

/// A short segment that sits mostly inside a longer one is the same stretch
/// detected twice. Give it the longer segment's speaker.
pub fn absorb_overlaps(segments: &[Segment]) -> Vec<Segment> {
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
    let mut speaker = ordered
        .iter()
        .map(|segment| segment.speaker)
        .collect::<Vec<_>>();
    for index in 0..ordered.len() {
        let duration = ordered[index].end - ordered[index].start;
        let mut host: Option<usize> = None;
        for other in 0..ordered.len() {
            if other == index {
                continue;
            }
            let other_duration = ordered[other].end - ordered[other].start;
            if other_duration <= duration {
                continue;
            }
            let overlap = overlap_seconds(&ordered[index], &ordered[other]);
            if overlap / duration >= 0.5 {
                host = Some(other);
                break;
            }
        }
        if let Some(other) = host {
            speaker[index] = ordered[other].speaker;
        }
    }
    for (segment, id) in ordered.iter_mut().zip(speaker) {
        segment.speaker = id;
    }
    ordered
}

/// A speaker with only a short fragment adopts the nearest longer speaker
/// when their voice embeddings are close. Two long turns are left alone.
pub fn link_short_speakers(
    segments: &[Segment],
    prints: &[(i32, Vec<f32>)],
    max_short_seconds: f64,
    max_distance: f64,
) -> Vec<Segment> {
    let mut duration = std::collections::HashMap::<i32, f64>::new();
    for segment in segments {
        *duration.entry(segment.speaker).or_insert(0.0) += segment.end - segment.start;
    }
    let mut relabel = std::collections::HashMap::<i32, i32>::new();
    for (&speaker, &seconds) in &duration {
        if seconds >= max_short_seconds {
            continue;
        }
        let Some(embedding) = print_of(prints, speaker) else {
            continue;
        };
        let mut best: Option<(i32, f64)> = None;
        for (&other, &other_seconds) in &duration {
            if other == speaker || other_seconds < max_short_seconds {
                continue;
            }
            let Some(other_embedding) = print_of(prints, other) else {
                continue;
            };
            let Some(distance) = cosine_distance(embedding, other_embedding) else {
                continue;
            };
            if best
                .as_ref()
                .is_none_or(|(_, best_distance)| distance < *best_distance)
            {
                best = Some((other, distance));
            }
        }
        if let Some((other, distance)) = best {
            if distance <= max_distance {
                relabel.insert(speaker, other);
            }
        }
    }
    segments
        .iter()
        .copied()
        .map(|mut segment| {
            if let Some(host) = relabel.get(&segment.speaker) {
                segment.speaker = *host;
            }
            segment
        })
        .collect()
}

fn print_of(prints: &[(i32, Vec<f32>)], speaker: i32) -> Option<&[f32]> {
    prints
        .iter()
        .find(|(id, _)| *id == speaker)
        .map(|(_, embedding)| embedding.as_slice())
}

pub fn cosine_distance(left: &[f32], right: &[f32]) -> Option<f64> {
    if left.is_empty() || left.len() != right.len() {
        return None;
    }
    let mut dot = 0.0_f64;
    let mut left_norm = 0.0_f64;
    let mut right_norm = 0.0_f64;
    for (a, b) in left.iter().zip(right) {
        let a = f64::from(*a);
        let b = f64::from(*b);
        dot += a * b;
        left_norm += a * a;
        right_norm += b * b;
    }
    if left_norm <= 0.0 || right_norm <= 0.0 {
        return None;
    }
    Some(1.0 - dot / (left_norm.sqrt() * right_norm.sqrt()))
}

fn overlap_seconds(left: &Segment, right: &Segment) -> f64 {
    let start = left.start.max(right.start);
    let end = left.end.min(right.end);
    (end - start).max(0.0)
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
    fn overlap_inside_a_longer_segment_keeps_that_speaker() {
        let segments = vec![seg(13.04, 14.86, 2), seg(14.22, 15.12, 3)];
        let absorbed = absorb_overlaps(&segments);
        assert!(absorbed.iter().all(|segment| segment.speaker == 2));
    }

    #[test]
    fn short_fragment_joins_the_closer_long_speaker() {
        let segments = vec![seg(0.0, 12.0, 0), seg(16.0, 16.8, 1), seg(51.0, 65.0, 2)];
        let prints = vec![
            (0, vec![1.0, 0.0]),
            (1, vec![0.15, 0.99]),
            (2, vec![0.0, 1.0]),
        ];
        let linked = link_short_speakers(&segments, &prints, 2.0, 0.65);
        assert_eq!(linked[1].speaker, 2);
        assert_eq!(linked[0].speaker, 0);
        assert_eq!(linked[2].speaker, 2);
    }

    #[test]
    fn distant_short_fragment_stays_its_own_speaker() {
        let segments = vec![seg(0.0, 12.0, 0), seg(16.0, 16.8, 1)];
        let prints = vec![(0, vec![1.0, 0.0]), (1, vec![-1.0, 0.0])];
        let linked = link_short_speakers(&segments, &prints, 2.0, 0.65);
        assert_eq!(linked[1].speaker, 1);
    }

    #[test]
    fn two_long_speakers_are_not_linked() {
        let segments = vec![seg(0.0, 12.0, 0), seg(20.0, 32.0, 1)];
        let prints = vec![(0, vec![1.0, 0.0]), (1, vec![0.95, 0.1])];
        let linked = link_short_speakers(&segments, &prints, 2.0, 0.65);
        assert_eq!(linked, segments);
    }

    #[test]
    fn separate_turns_are_not_absorbed() {
        let segments = vec![seg(0.0, 1.5, 0), seg(2.0, 3.5, 1)];
        assert_eq!(absorb_overlaps(&segments), segments);
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
