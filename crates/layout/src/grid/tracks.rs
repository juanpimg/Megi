use super::*;

/// Expands `auto-fill`/`auto-fit` repetitions for the available size and returns
/// the index range of the repeated tracks (used to collapse empty `auto-fit`
/// tracks after placement).
pub(super) fn expand_template(
    template: &megi_css::GridTemplate,
    available: f32,
    gap: f32,
) -> (Vec<TrackSize>, Option<(usize, usize)>) {
    let mut tracks = template.tracks.clone();
    let mut range = None;
    if let Some(AutoRepeat {
        kind,
        tracks: repeated,
    }) = &template.auto_repeat
    {
        let count = auto_repeat_count(repeated, available, gap);
        let start = tracks.len();
        for _ in 0..count {
            if tracks.len() + repeated.len() > MAX_EXPANDED_TRACKS {
                break;
            }
            tracks.extend_from_slice(repeated);
        }
        if *kind == AutoRepeatKind::Fit && tracks.len() > start {
            range = Some((start, tracks.len()));
        }
    }
    (tracks, range)
}

pub(super) fn auto_repeat_count(tracks: &[TrackSize], available: f32, gap: f32) -> usize {
    if tracks.is_empty() {
        return 1;
    }
    let mut size = 0.0_f32;
    for track in tracks {
        size += min_track_size(track, available);
    }
    let inner_gaps = gap * (tracks.len().saturating_sub(1) as f32);
    let unit = size + inner_gaps;
    if unit <= 0.0 {
        return 1;
    }
    let count = ((available + gap) / (unit + gap)).floor().max(1.0) as usize;
    count.min(MAX_EXPANDED_TRACKS)
}

pub(super) fn min_track_size(track: &TrackSize, reference: f32) -> f32 {
    match track {
        TrackSize::Fixed(length) => definite_length(*length, None, Some(reference)).unwrap_or(0.0),
        TrackSize::MinMax { min, .. } => match min {
            TrackMin::Fixed(length) => {
                definite_length(*length, None, Some(reference)).unwrap_or(0.0)
            }
            TrackMin::Auto => 0.0,
        },
        TrackSize::Fraction(_) | TrackSize::Auto => 0.0,
    }
}

pub(super) fn visible_gap_pairs(collapsed: &[bool]) -> usize {
    collapsed
        .windows(2)
        .filter(|pair| !pair[0] && !pair[1])
        .count()
}

/// Cumulative track offsets including the leading content-distribution offset and
/// skipping gutters adjacent to collapsed (`auto-fit`) tracks.
pub(super) fn track_offsets(sizes: &[f32], collapsed: &[bool], gap: f32, leading: f32) -> Vec<f32> {
    let mut offsets = Vec::with_capacity(sizes.len());
    let mut offset = leading;
    for (index, size) in sizes.iter().enumerate() {
        offsets.push(offset);
        offset += *size;
        if index + 1 < sizes.len() && !collapsed[index] && !collapsed[index + 1] {
            offset += gap;
        }
    }
    offsets
}

/// Size of the cells from `start` to `start + span`, including inner gutters and
/// excluding collapsed tracks.
pub(super) fn span_size(
    sizes: &[f32],
    collapsed: &[bool],
    start: usize,
    span: usize,
    gap: f32,
) -> f32 {
    let end = (start + span).min(sizes.len());
    let mut total = 0.0;
    for index in start..end {
        total += sizes[index];
        if index + 1 < end && !collapsed[index] && !collapsed[index + 1] {
            total += gap;
        }
    }
    total
}

/// Distributes leftover space along one axis per `justify-content`/`align-content`.
///
/// Returns `(leading_offset, extra_gap)`; `stretch` grows the automatic tracks in
/// place instead of adding offsets.
pub(super) fn content_distribution(
    distribution: GridContentDistribution,
    sizes: &mut [f32],
    collapsed: &[bool],
    remaining: f32,
    auto_tracks: &[(usize, f32)],
) -> (f32, f32) {
    let visible = collapsed.iter().filter(|value| !**value).count();
    match distribution {
        GridContentDistribution::Stretch => {
            let eligible: Vec<usize> = auto_tracks
                .iter()
                .map(|(index, _)| *index)
                .filter(|index| *index < collapsed.len() && !collapsed[*index])
                .collect();
            if remaining > 0.0 && !eligible.is_empty() {
                let share = remaining / eligible.len() as f32;
                for index in eligible {
                    sizes[index] += share;
                }
            }
            (0.0, 0.0)
        }
        GridContentDistribution::Start => (0.0, 0.0),
        GridContentDistribution::End => (remaining, 0.0),
        GridContentDistribution::Center => (remaining / 2.0, 0.0),
        GridContentDistribution::SpaceBetween => {
            if visible > 1 {
                (0.0, remaining / (visible - 1) as f32)
            } else {
                (0.0, 0.0)
            }
        }
        GridContentDistribution::SpaceAround => {
            if visible > 0 {
                (remaining / (2 * visible) as f32, remaining / visible as f32)
            } else {
                (0.0, 0.0)
            }
        }
        GridContentDistribution::SpaceEvenly => {
            if visible > 0 {
                let share = remaining / (visible + 1) as f32;
                (share, share)
            } else {
                (0.0, 0.0)
            }
        }
    }
}
