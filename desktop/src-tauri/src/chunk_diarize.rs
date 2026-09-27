//! Experimental diarization while recording. Each closed chunk is labeled and
//! transcribed immediately. Speaker numbers from a chunk are local; voice
//! prints are clustered once at the end so the same person keeps one label.
//!
//! Delete this module, `ChunkRun`, and `diarization_chunked` to remove the
//! experiment. The whole-session path does not call in here.

use crate::speakers::{self, Segment};

/// Audio repeated at the start of the next chunk so a turn cut by the rotate
/// is not transcribed twice and still has a voice print.
pub const OVERLAP_SECONDS: f64 = 2.0;
/// Across chunks, one 8-second window per voice. Sherpa's 0.5 is for many
/// windows at once and left the same person as a new speaker in the next chunk.
pub const CLUSTER_DISTANCE: f64 = 0.65;
/// Inside one chunk, join only an obvious over-split. Farther than this, the
/// diarizer's split stands so two people are not glued back together.
const SAME_CHUNK_DISTANCE: f64 = 0.40;
/// A turn with no voice print borrows the nearest labeled turn inside this gap.
const UNPRINTED_GAP_SECONDS: f64 = 8.0;
/// A voice with less speech than this is a scrap, not another person. The three
/// long voices in a session are well above it.
const BRIEF_SECONDS: f64 = 12.0;
/// A short window is a noisy print, so a scrap may sit farther from its person
/// than two long turns of the same voice.
const BRIEF_DISTANCE: f64 = 0.80;

/// One already-transcribed piece of a chunk, on the session clock.
#[derive(Clone, Debug)]
pub struct KeptTurn {
    pub start: f64,
    pub end: f64,
    pub text: String,
    /// Index into the print list for this session. `None` never joins another voice.
    pub print_id: Option<usize>,
}

/// Voice print taken from one chunk. Speakers that share `chunk` join only when
/// they are closer than [SAME_CHUNK_DISTANCE]; otherwise that chunk's split stands.
#[derive(Clone, Debug)]
pub struct VoicePrint {
    pub embedding: Vec<f32>,
    pub chunk: u32,
}

/// Part of `segment` that this chunk owns.
/// `prefix` is the repeated head from the previous chunk.
/// `cut` is the local time where the next chunk takes over (`duration` on the final chunk).
pub fn owned_span(start: f64, end: f64, prefix: f64, cut: f64) -> Option<(f64, f64)> {
    if !start.is_finite() || !end.is_finite() || !(end > start) {
        return None;
    }
    let from = start.max(prefix);
    let to = end.min(cut);
    if to > from {
        Some((from, to))
    } else {
        None
    }
}

/// Join voices whose cosine distance is within `max_distance`, closest pairs first.
/// Prints from the same chunk join only inside [SAME_CHUNK_DISTANCE]. Labels are
/// 0-based in order of first appearance. An empty print stays alone.
pub fn cluster_prints(prints: &[VoicePrint], max_distance: f64) -> Vec<i32> {
    cluster_voices(prints, max_distance, -1)
}

/// `speakers` is 2, 3, or -1. A fixed count keeps merging the closest remaining
/// pair until that many voices are left. A split inside one chunk is not undone.
fn cluster_voices(prints: &[VoicePrint], max_distance: f64, speakers: i32) -> Vec<i32> {
    let n = prints.len();
    let mut parent: Vec<usize> = (0..n).collect();
    let mut pairs = Vec::new();
    for i in 0..n {
        if prints[i].embedding.is_empty() {
            continue;
        }
        for j in (i + 1)..n {
            if prints[j].embedding.is_empty() {
                continue;
            }
            let Some(distance) =
                speakers::cosine_distance(&prints[i].embedding, &prints[j].embedding)
            else {
                continue;
            };
            let limit = if prints[i].chunk == prints[j].chunk {
                SAME_CHUNK_DISTANCE
            } else {
                max_distance
            };
            if distance <= limit {
                pairs.push((distance, i, j));
            }
        }
    }
    pairs.sort_by(|left, right| {
        left.0
            .partial_cmp(&right.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(left.1.cmp(&right.1))
            .then(left.2.cmp(&right.2))
    });
    for (_, i, j) in pairs {
        if split_blocks(&mut parent, prints, i, j) {
            continue;
        }
        let root_j = find(&mut parent, j);
        let root_i = find(&mut parent, i);
        parent[root_j] = root_i;
    }
    if speakers == 2 || speakers == 3 {
        merge_toward(&mut parent, prints, speakers as usize);
    }
    let mut label_of_root = std::collections::HashMap::<usize, i32>::new();
    let mut next = 0_i32;
    let mut labels = Vec::with_capacity(n);
    for i in 0..n {
        let root = find(&mut parent, i);
        let label = *label_of_root.entry(root).or_insert_with(|| {
            let id = next;
            next += 1;
            id
        });
        labels.push(label);
    }
    labels
}

/// Merge the closest allowed pair until `speakers` clusters remain.
/// Stops early when every leftover pair would undo a split inside one chunk.
fn merge_toward(parent: &mut [usize], prints: &[VoicePrint], speakers: usize) {
    loop {
        if root_count(parent) <= speakers {
            break;
        }
        let mut best: Option<(f64, usize, usize)> = None;
        for i in 0..prints.len() {
            if prints[i].embedding.is_empty() {
                continue;
            }
            for j in (i + 1)..prints.len() {
                if prints[j].embedding.is_empty() {
                    continue;
                }
                let left = find(parent, i);
                let right = find(parent, j);
                if left == right || split_blocks(parent, prints, i, j) {
                    continue;
                }
                let Some(distance) =
                    speakers::cosine_distance(&prints[i].embedding, &prints[j].embedding)
                else {
                    continue;
                };
                let replace = match best {
                    None => true,
                    Some((best_distance, best_i, best_j)) => {
                        distance < best_distance
                            || (distance == best_distance && (i, j) < (best_i, best_j))
                    }
                };
                if replace {
                    best = Some((distance, i, j));
                }
            }
        }
        let Some((_, i, j)) = best else {
            break;
        };
        let root_j = find(parent, j);
        let root_i = find(parent, i);
        parent[root_j] = root_i;
    }
}

fn root_count(parent: &mut [usize]) -> usize {
    let mut seen = vec![false; parent.len()];
    let mut count = 0;
    for index in 0..parent.len() {
        let root = find(parent, index);
        if !seen[root] {
            seen[root] = true;
            count += 1;
        }
    }
    count
}

fn find(parent: &mut [usize], mut index: usize) -> usize {
    while parent[index] != index {
        parent[index] = parent[parent[index]];
        index = parent[index];
    }
    index
}

/// True when joining `i` and `j` would put two voices from one chunk together
/// even though that chunk kept them apart.
fn split_blocks(parent: &mut [usize], prints: &[VoicePrint], i: usize, j: usize) -> bool {
    let root_i = find(parent, i);
    let root_j = find(parent, j);
    if root_i == root_j {
        return true;
    }
    let left: Vec<usize> = (0..prints.len())
        .filter(|&index| find(parent, index) == root_i)
        .collect();
    let right: Vec<usize> = (0..prints.len())
        .filter(|&index| find(parent, index) == root_j)
        .collect();
    for &a in &left {
        for &b in &right {
            if prints[a].chunk != prints[b].chunk {
                continue;
            }
            let Some(distance) =
                speakers::cosine_distance(&prints[a].embedding, &prints[b].embedding)
            else {
                return true;
            };
            if distance > SAME_CHUNK_DISTANCE {
                return true;
            }
        }
    }
    false
}

/// Map kept turns onto global speaker ids, ready for `speakers::format`.
/// A turn with no print takes the nearest labeled turn, so a short «угу»
/// does not become its own speaker.
pub fn label_turns(turns: &[KeptTurn], prints: &[VoicePrint]) -> (Vec<Segment>, Vec<String>) {
    label_turns_for(turns, prints, -1)
}

/// `speakers` is 2, 3, or -1 (automatic). A fixed count is a ceiling for this
/// session: extra voices join the closest one, and one voice is not split apart.
pub fn label_turns_for(
    turns: &[KeptTurn],
    prints: &[VoicePrint],
    speakers: i32,
) -> (Vec<Segment>, Vec<String>) {
    let cluster = cluster_voices(prints, CLUSTER_DISTANCE, speakers);
    let mut assigned: Vec<Option<i32>> = turns
        .iter()
        .map(|turn| turn.print_id.and_then(|id| cluster.get(id).copied()))
        .collect();
    absorb_brief_speakers(turns, prints, &mut assigned);
    for index in 0..turns.len() {
        if assigned[index].is_some() {
            continue;
        }
        if let Some(speaker) = nearest_speaker(turns, &assigned, index) {
            assigned[index] = Some(speaker);
        }
    }
    let mut orphan = assigned
        .iter()
        .flatten()
        .copied()
        .max()
        .map(|id| id + 1)
        .unwrap_or(0);
    let mut segments = Vec::new();
    let mut texts = Vec::new();
    for (turn, speaker) in turns.iter().zip(assigned) {
        let text = turn.text.trim();
        if text.is_empty() || !(turn.end > turn.start) {
            continue;
        }
        let speaker = speaker.unwrap_or_else(|| {
            let id = orphan;
            orphan += 1;
            id
        });
        segments.push(Segment {
            start: turn.start,
            end: turn.end,
            speaker,
        });
        texts.push(text.to_string());
    }
    (segments, texts)
}

/// Give a voice under [BRIEF_SECONDS] the nearest long voice within [BRIEF_DISTANCE].
fn absorb_brief_speakers(turns: &[KeptTurn], prints: &[VoicePrint], assigned: &mut [Option<i32>]) {
    let mut duration = std::collections::HashMap::<i32, f64>::new();
    for (turn, speaker) in turns.iter().zip(assigned.iter()) {
        let Some(speaker) = speaker else {
            continue;
        };
        *duration.entry(*speaker).or_insert(0.0) += (turn.end - turn.start).max(0.0);
    }
    let long: Vec<i32> = duration
        .iter()
        .filter(|(_, seconds)| **seconds >= BRIEF_SECONDS)
        .map(|(speaker, _)| *speaker)
        .collect();
    if long.is_empty() {
        return;
    }
    let brief: Vec<i32> = duration
        .iter()
        .filter(|(_, seconds)| **seconds < BRIEF_SECONDS)
        .map(|(speaker, _)| *speaker)
        .collect();
    for speaker in brief {
        let Some(host) = nearest_long_voice(speaker, &long, turns, prints, assigned) else {
            continue;
        };
        for slot in assigned.iter_mut() {
            if *slot == Some(speaker) {
                *slot = Some(host);
            }
        }
    }
}

fn nearest_long_voice(
    speaker: i32,
    long: &[i32],
    turns: &[KeptTurn],
    prints: &[VoicePrint],
    assigned: &[Option<i32>],
) -> Option<i32> {
    let mut best: Option<(f64, i32)> = None;
    for &host in long {
        let Some(distance) = voice_distance(speaker, host, turns, prints, assigned) else {
            continue;
        };
        if distance > BRIEF_DISTANCE {
            continue;
        }
        if best
            .as_ref()
            .is_none_or(|(best_distance, _)| distance < *best_distance)
        {
            best = Some((distance, host));
        }
    }
    best.map(|(_, host)| host)
}

fn voice_distance(
    left: i32,
    right: i32,
    turns: &[KeptTurn],
    prints: &[VoicePrint],
    assigned: &[Option<i32>],
) -> Option<f64> {
    let mut best: Option<f64> = None;
    for (left_index, left_turn) in turns.iter().enumerate() {
        if assigned[left_index] != Some(left) {
            continue;
        }
        let Some(left_print) = left_turn.print_id.and_then(|id| prints.get(id)) else {
            continue;
        };
        if left_print.embedding.is_empty() {
            continue;
        }
        for (right_index, right_turn) in turns.iter().enumerate() {
            if assigned[right_index] != Some(right) {
                continue;
            }
            let Some(right_print) = right_turn.print_id.and_then(|id| prints.get(id)) else {
                continue;
            };
            let Some(distance) =
                speakers::cosine_distance(&left_print.embedding, &right_print.embedding)
            else {
                continue;
            };
            if best.is_none_or(|best_distance| distance < best_distance) {
                best = Some(distance);
            }
        }
    }
    best
}

fn nearest_speaker(turns: &[KeptTurn], assigned: &[Option<i32>], index: usize) -> Option<i32> {
    let mid = midpoint(turns[index].start, turns[index].end)?;
    let mut best: Option<(f64, i32)> = None;
    for (other, speaker) in turns.iter().zip(assigned) {
        let Some(speaker) = speaker else {
            continue;
        };
        let Some(other_mid) = midpoint(other.start, other.end) else {
            continue;
        };
        let gap = (mid - other_mid).abs();
        if gap > UNPRINTED_GAP_SECONDS {
            continue;
        }
        if best.as_ref().is_none_or(|(best_gap, _)| gap < *best_gap) {
            best = Some((gap, *speaker));
        }
    }
    best.map(|(_, speaker)| speaker)
}

fn midpoint(start: f64, end: f64) -> Option<f64> {
    if start.is_finite() && end.is_finite() && end > start {
        Some((start + end) / 2.0)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn print(embedding: Vec<f32>, chunk: u32) -> VoicePrint {
        VoicePrint { embedding, chunk }
    }

    #[test]
    fn overlap_head_and_tail_are_not_owned() {
        assert_eq!(owned_span(0.0, 1.5, 2.0, 178.0), None);
        assert_eq!(owned_span(179.0, 180.0, 2.0, 178.0), None);
        assert_eq!(owned_span(10.0, 40.0, 2.0, 178.0), Some((10.0, 40.0)));
        assert_eq!(owned_span(0.5, 12.0, 2.0, 178.0), Some((2.0, 12.0)));
        assert_eq!(owned_span(170.0, 180.0, 2.0, 178.0), Some((170.0, 178.0)));
    }

    #[test]
    fn final_chunk_keeps_through_the_end() {
        assert_eq!(owned_span(1.0, 9.0, 2.0, 9.0), Some((2.0, 9.0)));
    }

    #[test]
    fn identical_prints_from_two_chunks_are_one_speaker() {
        let prints = vec![
            print(vec![1.0, 0.0], 0),
            print(vec![0.0, 1.0], 1),
            print(vec![1.0, 0.0], 2),
        ];
        let labels = cluster_prints(&prints, CLUSTER_DISTANCE);
        assert_eq!(labels[0], labels[2]);
        assert_ne!(labels[0], labels[1]);
    }

    #[test]
    fn oversplit_in_one_chunk_joins_back() {
        // Distance is about 0.05: the same voice, cut in two by the chunk model.
        let prints = vec![print(vec![1.0, 0.0], 0), print(vec![0.95, 0.31], 0)];
        let turns = vec![
            KeptTurn {
                start: 0.0,
                end: 5.0,
                text: "алло".into(),
                print_id: Some(0),
            },
            KeptTurn {
                start: 6.0,
                end: 10.0,
                text: "да".into(),
                print_id: Some(1),
            },
        ];
        let (segments, texts) = label_turns(&turns, &prints);
        assert_eq!(speakers::format(&segments, &texts), "алло да");
    }

    #[test]
    fn a_bridge_print_does_not_merge_speakers_from_the_same_chunk() {
        let prints = vec![
            print(vec![1.0, 0.0], 0),
            print(vec![0.0, 1.0], 0),
            print(vec![1.0, 1.0], 1),
        ];
        let labels = cluster_prints(&prints, CLUSTER_DISTANCE);
        assert_eq!(labels[0], labels[2]);
        assert_ne!(labels[0], labels[1]);
    }

    #[test]
    fn one_voice_across_chunks_stays_a_plain_line() {
        let prints = vec![print(vec![1.0, 0.0], 0), print(vec![1.0, 0.0], 1)];
        let turns = vec![
            KeptTurn {
                start: 0.0,
                end: 10.0,
                text: "первый кусок".into(),
                print_id: Some(0),
            },
            KeptTurn {
                start: 178.0,
                end: 190.0,
                text: "второй кусок".into(),
                print_id: Some(1),
            },
        ];
        let (segments, texts) = label_turns(&turns, &prints);
        let text = speakers::format(&segments, &texts);
        assert_eq!(text, "первый кусок второй кусок");
        assert!(!text.contains("Спикер"));
    }

    #[test]
    fn same_voice_past_sherpa_threshold_joins_across_chunks() {
        // Cosine distance 0.55. Sherpa's 0.5 would leave a second speaker.
        let prints = vec![print(vec![1.0, 0.0], 0), print(vec![0.45, 0.893], 1)];
        let labels = cluster_prints(&prints, CLUSTER_DISTANCE);
        assert_eq!(labels[0], labels[1]);
    }

    #[test]
    fn same_chunk_split_past_the_tight_limit_stays() {
        let prints = vec![print(vec![1.0, 0.0], 0), print(vec![0.45, 0.893], 0)];
        let labels = cluster_prints(&prints, CLUSTER_DISTANCE);
        assert_ne!(labels[0], labels[1]);
    }

    #[test]
    fn a_short_line_without_a_print_joins_the_neighbor() {
        let prints = vec![print(vec![1.0, 0.0], 0)];
        let turns = vec![
            KeptTurn {
                start: 0.0,
                end: 8.0,
                text: "длинная реплика".into(),
                print_id: Some(0),
            },
            KeptTurn {
                start: 8.2,
                end: 8.8,
                text: "угу".into(),
                print_id: None,
            },
        ];
        let (segments, texts) = label_turns(&turns, &prints);
        assert_eq!(speakers::format(&segments, &texts), "длинная реплика угу");
    }

    #[test]
    fn a_short_scrap_joins_the_nearest_long_voice() {
        // Distance is about 0.55, past the same-chunk limit, under the scrap gate.
        let prints = vec![print(vec![1.0, 0.0], 0), print(vec![0.45, 0.893], 0)];
        let turns = vec![
            KeptTurn {
                start: 0.0,
                end: 20.0,
                text: "длинная реплика".into(),
                print_id: Some(0),
            },
            KeptTurn {
                start: 20.0,
                end: 23.0,
                text: "обрывок".into(),
                print_id: Some(1),
            },
        ];
        let (segments, texts) = label_turns(&turns, &prints);
        assert_eq!(
            speakers::format(&segments, &texts),
            "длинная реплика обрывок"
        );
    }

    #[test]
    fn a_far_scrap_stays_its_own_speaker() {
        let prints = vec![print(vec![1.0, 0.0], 0), print(vec![0.05, 0.999], 1)];
        let turns = vec![
            KeptTurn {
                start: 0.0,
                end: 20.0,
                text: "длинная".into(),
                print_id: Some(0),
            },
            KeptTurn {
                start: 21.0,
                end: 24.0,
                text: "чужой".into(),
                print_id: Some(1),
            },
        ];
        let (segments, texts) = label_turns(&turns, &prints);
        assert_eq!(
            speakers::format(&segments, &texts),
            "Спикер 1: длинная\n\nСпикер 2: чужой"
        );
    }

    #[test]
    fn two_long_voices_stay_split_when_a_scrap_would_have_joined() {
        let prints = vec![print(vec![1.0, 0.0], 0), print(vec![0.45, 0.893], 0)];
        let turns = vec![
            KeptTurn {
                start: 0.0,
                end: 20.0,
                text: "первый".into(),
                print_id: Some(0),
            },
            KeptTurn {
                start: 21.0,
                end: 40.0,
                text: "второй".into(),
                print_id: Some(1),
            },
        ];
        let (segments, texts) = label_turns(&turns, &prints);
        assert_eq!(
            speakers::format(&segments, &texts),
            "Спикер 1: первый\n\nСпикер 2: второй"
        );
    }

    #[test]
    fn two_voices_keep_first_appearance_order() {
        let prints = vec![print(vec![1.0, 0.0], 0), print(vec![0.0, 1.0], 0)];
        let turns = vec![
            KeptTurn {
                start: 0.0,
                end: 5.0,
                text: "алло".into(),
                print_id: Some(0),
            },
            KeptTurn {
                start: 6.0,
                end: 10.0,
                text: "да".into(),
                print_id: Some(1),
            },
        ];
        let (segments, texts) = label_turns(&turns, &prints);
        assert_eq!(
            speakers::format(&segments, &texts),
            "Спикер 1: алло\n\nСпикер 2: да"
        );
    }

    #[test]
    fn a_fixed_count_joins_the_closest_extra_voice() {
        // Distances: A–B ≈ 0.70, B–C ≈ 1.3, A–C = 2. All above the auto threshold.
        let prints = vec![
            print(vec![1.0, 0.0], 0),
            print(vec![0.3, 0.954], 1),
            print(vec![-1.0, 0.0], 2),
        ];
        let auto = cluster_prints(&prints, CLUSTER_DISTANCE);
        assert_eq!(auto, vec![0, 1, 2]);
        let capped = cluster_voices(&prints, CLUSTER_DISTANCE, 2);
        assert_eq!(capped[0], capped[1]);
        assert_ne!(capped[0], capped[2]);
    }

    #[test]
    fn a_fixed_count_does_not_undo_a_split_inside_one_chunk() {
        let prints = vec![
            print(vec![1.0, 0.0], 0),
            print(vec![0.3, 0.954], 0),
            print(vec![-1.0, 0.0], 0),
        ];
        let capped = cluster_voices(&prints, CLUSTER_DISTANCE, 2);
        assert_eq!(capped, vec![0, 1, 2]);
    }
}
