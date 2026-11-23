//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;

/// Parses one `<grid-line>`: `auto`, a signed integer or `span N`.
pub(crate) fn parse_grid_line(raw: &str) -> GridLine {
    let raw = raw.trim();
    if raw.is_empty() || raw.eq_ignore_ascii_case("auto") {
        return GridLine::Auto;
    }
    let lower = raw.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("span") {
        return rest
            .trim()
            .parse::<u32>()
            .ok()
            .filter(|span| *span > 0)
            .map(GridLine::Span)
            .unwrap_or(GridLine::Auto);
    }
    if let Ok(line) = lower.parse::<i32>() {
        return if line != 0 {
            GridLine::Line(line)
        } else {
            GridLine::Auto
        };
    }
    // `<custom-ident>`: named line or area-derived name.
    if lower
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        GridLine::Name(lower)
    } else {
        GridLine::Auto
    }
}

pub(crate) fn parse_align_items(value: &str) -> AlignItems {
    match value.to_ascii_lowercase().as_str() {
        "start" | "flex-start" | "left" | "self-start" => AlignItems::FlexStart,
        "end" | "flex-end" | "right" | "self-end" => AlignItems::FlexEnd,
        "center" => AlignItems::Center,
        "baseline" => AlignItems::Baseline,
        _ => AlignItems::Stretch,
    }
}

pub(crate) fn parse_align_self(value: &str) -> AlignSelf {
    match value.to_ascii_lowercase().as_str() {
        "start" | "flex-start" | "left" | "self-start" => AlignSelf::FlexStart,
        "end" | "flex-end" | "right" | "self-end" => AlignSelf::FlexEnd,
        "center" => AlignSelf::Center,
        "baseline" => AlignSelf::Baseline,
        "stretch" => AlignSelf::Stretch,
        _ => AlignSelf::Auto,
    }
}

/// Parses a grid track list (`none` yields an empty template).
pub(crate) fn parse_template(raw: &str, font_size: f32, root_font_size: f32) -> GridTemplate {
    let raw = raw.trim();
    if raw.is_empty() || raw.eq_ignore_ascii_case("none") {
        return GridTemplate::default();
    }
    let mut tracks = Vec::new();
    let mut auto_repeat = None;
    let mut line_names: Vec<Vec<String>> = vec![Vec::new()];
    parse_track_list(
        raw,
        font_size,
        root_font_size,
        &mut tracks,
        &mut auto_repeat,
        &mut line_names,
    );
    tracks.truncate(MAX_TRACKS);
    line_names.truncate(tracks.len() + 1);
    while line_names.len() < tracks.len() + 1 {
        line_names.push(Vec::new());
    }
    GridTemplate {
        tracks,
        auto_repeat,
        line_names,
    }
}

pub(crate) fn parse_track_list(
    raw: &str,
    font_size: f32,
    root_font_size: f32,
    tracks: &mut Vec<TrackSize>,
    auto_repeat: &mut Option<AutoRepeat>,
    line_names: &mut Vec<Vec<String>>,
) {
    let mut rest = raw;
    loop {
        rest = rest.trim_start_matches(|ch: char| ch.is_whitespace() || ch == ',');
        if rest.is_empty() || tracks.len() >= MAX_TRACKS {
            return;
        }
        let (token, consumed) = split_token(rest);
        if token.is_empty() {
            return;
        }
        rest = &rest[consumed..];
        if token.starts_with('[') && token.ends_with(']') {
            // `[name name2]` attaches names to the current boundary.
            let names = &token[1..token.len() - 1];
            if let Some(boundary) = line_names.last_mut() {
                for name in names.split_whitespace() {
                    if !name.is_empty() && boundary.len() < 8 {
                        boundary.push(name.to_owned());
                    }
                }
            }
            continue;
        }
        if let Some(inner) = token
            .strip_prefix("repeat(")
            .or_else(|| token.strip_prefix("REPEAT("))
        {
            let body = inner.trim_end_matches(')');
            if let Some((count, list)) = body.split_once(',') {
                let count = count.trim();
                let list = list.trim();
                let mut single = Vec::new();
                let mut nested = None;
                let mut nested_names = vec![Vec::new()];
                parse_track_list(
                    list,
                    font_size,
                    root_font_size,
                    &mut single,
                    &mut nested,
                    &mut nested_names,
                );
                let lowered = count.to_ascii_lowercase();
                if lowered == "auto-fill" || lowered == "auto-fit" {
                    // Only the first auto repetition is kept (documented MVP limit).
                    if auto_repeat.is_none() {
                        *auto_repeat = Some(AutoRepeat {
                            kind: if lowered == "auto-fit" {
                                AutoRepeatKind::Fit
                            } else {
                                AutoRepeatKind::Fill
                            },
                            tracks: single,
                        });
                    }
                } else {
                    let count = count.parse::<usize>().unwrap_or(0).min(MAX_REPEAT);
                    for _ in 0..count {
                        if tracks.len() + single.len() > MAX_TRACKS {
                            break;
                        }
                        tracks.extend_from_slice(&single);
                        for _ in 0..single.len() {
                            line_names.push(Vec::new());
                        }
                    }
                }
            }
            continue;
        }
        if let Some(track) = parse_track(token, font_size, root_font_size) {
            tracks.push(track);
            line_names.push(Vec::new());
        }
    }
}

/// Splits one token, keeping balanced parentheses for `repeat()`.
pub(crate) fn split_token(raw: &str) -> (&str, usize) {
    let bytes = raw.as_bytes();
    let mut depth = 0usize;
    for (index, byte) in bytes.iter().enumerate() {
        match byte {
            b'(' | b'[' => depth += 1,
            b')' | b']' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => return (raw[..index].trim(), index),
            b' ' | b'\t' | b'\n' if depth == 0 => return (raw[..index].trim(), index),
            _ => {}
        }
    }
    (raw.trim(), raw.len())
}

pub(crate) fn parse_track(token: &str, font_size: f32, root_font_size: f32) -> Option<TrackSize> {
    let token = token.trim();
    if token.is_empty() {
        return None;
    }
    if token.eq_ignore_ascii_case("auto")
        || token.eq_ignore_ascii_case("min-content")
        || token.eq_ignore_ascii_case("max-content")
    {
        return Some(TrackSize::Auto);
    }
    if let Some(inner) = token
        .strip_prefix("minmax(")
        .or_else(|| token.strip_prefix("MINMAX("))
    {
        return parse_minmax(inner.trim_end_matches(')'), font_size, root_font_size);
    }
    if let Some(fraction) = token
        .strip_suffix("fr")
        .or_else(|| token.strip_suffix("FR"))
    {
        let value = fraction.trim().parse::<f32>().ok()?;
        return value
            .is_finite()
            .then(|| TrackSize::Fraction(value.max(0.0)));
    }
    Length::parse(token, font_size, root_font_size).map(TrackSize::Fixed)
}

/// Parses `minmax(<min>, <max>)`.
pub(crate) fn parse_minmax(body: &str, font_size: f32, root_font_size: f32) -> Option<TrackSize> {
    let (min, max) = body.split_once(',')?;
    let min = min.trim();
    let max = max.trim();
    let min = if min.eq_ignore_ascii_case("auto") {
        TrackMin::Auto
    } else {
        TrackMin::Fixed(Length::parse(min, font_size, root_font_size)?)
    };
    let max = if max.eq_ignore_ascii_case("auto") {
        TrackMax::Auto
    } else if let Some(fraction) = max.strip_suffix("fr").or_else(|| max.strip_suffix("FR")) {
        let value = fraction.trim().parse::<f32>().ok()?;
        // #2139: reject non-finite <flex> values like 1e400fr (CSS Grid §7).
        if !value.is_finite() {
            return None;
        }
        TrackMax::Fraction(value.max(0.0))
    } else {
        TrackMax::Fixed(Length::parse(max, font_size, root_font_size)?)
    };
    Some(TrackSize::MinMax { min, max })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::rule::Declaration;

    fn declaration(name: &str, raw: &str) -> Declaration {
        Declaration {
            name: name.to_owned(),
            value: Vec::new(),
            important: false,
            raw_value: raw.to_owned(),
        }
    }

    fn compute(pairs: &[(&str, &str)]) -> ComputedGrid {
        let declarations: Vec<Declaration> = pairs
            .iter()
            .map(|(name, value)| declaration(name, value))
            .collect();
        let entries: Vec<(&str, &Declaration)> = pairs
            .iter()
            .zip(declarations.iter())
            .map(|((name, _), declaration)| (*name, declaration))
            .collect();
        compute_grid(&entries, 16.0, 16.0)
    }

    #[test]
    fn test_defaults_are_auto_and_stretch() {
        let grid = compute(&[]);
        assert!(grid.template_columns.is_none());
        assert_eq!(grid.justify_items, AlignItems::Stretch);
        assert_eq!(grid.justify_self, AlignSelf::Auto);
    }

    #[test]
    fn test_parse_lengths_percent_fr_and_repeat() {
        let grid = compute(&[
            ("grid-template-columns", "100px 1fr 2fr 25%"),
            ("grid-template-rows", "repeat(2, 40px) auto"),
            ("grid-auto-rows", "30px"),
        ]);
        assert_eq!(
            grid.template_columns.tracks,
            vec![
                TrackSize::Fixed(Length::Px(100.0)),
                TrackSize::Fraction(1.0),
                TrackSize::Fraction(2.0),
                TrackSize::Fixed(Length::Percent(25.0)),
            ]
        );
        assert_eq!(
            grid.template_rows.tracks,
            vec![
                TrackSize::Fixed(Length::Px(40.0)),
                TrackSize::Fixed(Length::Px(40.0)),
                TrackSize::Auto,
            ]
        );
        assert_eq!(
            grid.auto_rows.tracks,
            vec![TrackSize::Fixed(Length::Px(30.0))]
        );
    }

    #[test]
    fn test_alignment_and_inline_grid_flag() {
        let grid = compute(&[
            ("display", "inline-grid"),
            ("justify-items", "center"),
            ("justify-self", "end"),
        ]);
        assert!(grid.is_inline_grid);
        assert_eq!(grid.justify_items, AlignItems::Center);
        assert_eq!(grid.justify_self, AlignSelf::FlexEnd);
    }
}
