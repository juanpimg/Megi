//! `srcset`/`sizes` parsing and responsive candidate selection (WHATWG HTML §4.8.4).

use super::types::{SrcsetCandidate, SrcsetDescriptor};

/// Sanitizes device pixel ratio (finite and positive; falls back to `1.0`).
pub fn sanitize_dpr(dpr: f32) -> f32 {
    if dpr.is_finite() && dpr > 0.0 {
        dpr
    } else {
        1.0
    }
}

/// Parses an HTML `srcset` microsyntax string (WHATWG HTML §4.8.4.2).
///
/// Supports commas inside `data:` URLs cleanly by splitting URL tokens on ASCII
/// whitespace (stripping trailing `,` directly attached to a descriptor-less URL)
/// and scanning descriptor lists up to the next top-level comma.
/// Rejects malformed descriptors and keeps the candidate set homogeneous
/// (cannot mix `w` and `x` / default descriptors).
pub fn parse_srcset(input: &str) -> Vec<SrcsetCandidate> {
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut pos = 0usize;
    let mut raw_entries: Vec<(String, Option<SrcsetDescriptor>)> = Vec::new();

    while pos < len {
        // 1. Skip leading ASCII whitespace and commas.
        while pos < len && (bytes[pos].is_ascii_whitespace() || bytes[pos] == b',') {
            pos += 1;
        }
        if pos >= len {
            break;
        }

        // 2. Scan candidate URL (non-ASCII-whitespace).
        let url_start = pos;
        while pos < len && !bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        let raw_url = &input[url_start..pos];

        // Per HTML §4.8.4.2: if the URL ends with `,`, trailing commas delimit the
        // candidate and there are zero descriptors.
        if raw_url.ends_with(',') {
            let trimmed_url = raw_url.trim_end_matches(',');
            if !trimmed_url.is_empty() {
                raw_entries.push((trimmed_url.to_string(), None));
            }
            continue;
        }

        // Skip whitespace before descriptors.
        while pos < len && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }

        // 3. Scan descriptor region up to next top-level comma (respecting parens).
        let desc_start = pos;
        let mut depth = 0usize;
        while pos < len {
            let b = bytes[pos];
            if b == b'(' {
                depth += 1;
            } else if b == b')' {
                depth = depth.saturating_sub(1);
            } else if b == b',' && depth == 0 {
                break;
            }
            pos += 1;
        }
        let desc_str = input[desc_start..pos].trim();
        if pos < len && bytes[pos] == b',' {
            pos += 1;
        }

        if desc_str.is_empty() {
            raw_entries.push((raw_url.to_string(), None));
            continue;
        }

        // HTML spec allows at most one width (`w`) or pixel density (`x`) descriptor token.
        let tokens: Vec<&str> = desc_str.split_ascii_whitespace().collect();
        if tokens.len() != 1 {
            continue;
        }
        let tok = tokens[0];
        if let Some(num) = tok.strip_suffix(['w', 'W']) {
            if let Ok(width) = num.parse::<u32>()
                && width > 0
                && !num.starts_with('+')
            {
                raw_entries.push((raw_url.to_string(), Some(SrcsetDescriptor::Width(width))));
            }
        } else if let Some(num) = tok.strip_suffix(['x', 'X'])
            && let Ok(density) = num.parse::<f32>()
            && density.is_finite()
            && density > 0.0
        {
            raw_entries.push((
                raw_url.to_string(),
                Some(SrcsetDescriptor::Density(density)),
            ));
        }
    }

    if raw_entries.is_empty() {
        return Vec::new();
    }

    // Spec §4.8.4.2.1: if any valid candidate specifies a width descriptor, all valid
    // candidates must specify a width descriptor (mixed x/omitted descriptors are dropped).
    let uses_width = raw_entries
        .iter()
        .any(|(_, d)| matches!(d, Some(SrcsetDescriptor::Width(_))));

    let mut out = Vec::with_capacity(raw_entries.len());
    for (url, desc) in raw_entries {
        if uses_width {
            if let Some(SrcsetDescriptor::Width(w)) = desc
                && !out.iter().any(|c: &SrcsetCandidate| {
                    matches!(c.descriptor, SrcsetDescriptor::Width(prev) if prev == w)
                })
            {
                out.push(SrcsetCandidate {
                    url,
                    descriptor: SrcsetDescriptor::Width(w),
                });
            }
        } else {
            let density = match desc {
                Some(SrcsetDescriptor::Density(d)) => d,
                None => 1.0,
                Some(SrcsetDescriptor::Width(_)) => continue,
            };
            if !out.iter().any(|c: &SrcsetCandidate| {
                matches!(c.descriptor, SrcsetDescriptor::Density(prev) if (prev - density).abs() < 1e-4)
            }) {
                out.push(SrcsetCandidate {
                    url,
                    descriptor: SrcsetDescriptor::Density(density),
                });
            }
        }
    }
    out
}

/// Parses and evaluates a `<source-size-value>` CSS length against the viewport metrics.
fn parse_size_length_px(token: &str, media: &megi_css::MediaContext) -> Option<f32> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return None;
    }
    let (vw, vh) = media.viewport.unwrap_or((800.0, 600.0));
    let lower = trimmed.to_ascii_lowercase();
    let parse_unit = |suffix: &str, scale: f32| -> Option<f32> {
        let num_str = lower.strip_suffix(suffix)?.trim();
        let val = num_str.parse::<f32>().ok()?;
        if val.is_finite() && val >= 0.0 {
            Some(val * scale)
        } else {
            None
        }
    };
    if lower == "0" {
        return Some(0.0);
    }
    parse_unit("px", 1.0)
        .or_else(|| parse_unit("vw", vw / 100.0))
        .or_else(|| parse_unit("vh", vh / 100.0))
        .or_else(|| parse_unit("vmin", vw.min(vh) / 100.0))
        .or_else(|| parse_unit("vmax", vw.max(vh) / 100.0))
        .or_else(|| parse_unit("em", 16.0))
        .or_else(|| parse_unit("rem", 16.0))
        .or_else(|| parse_unit("pt", 96.0 / 72.0))
        .or_else(|| parse_unit("pc", 16.0))
        .or_else(|| parse_unit("in", 96.0))
        .or_else(|| parse_unit("cm", 96.0 / 2.54))
        .or_else(|| parse_unit("mm", 96.0 / 25.4))
}

/// Evaluates an HTML `sizes` attribute against `media`, returning the target CSS width in px.
///
/// Falls back to the viewport width (`100vw`, default `800px` when viewport is unknown).
pub fn resolve_sizes(sizes_attr: Option<&str>, media: &megi_css::MediaContext) -> f32 {
    let default_width = media.viewport.map_or(800.0, |(w, _)| w.max(1.0));
    let Some(raw) = sizes_attr.map(str::trim).filter(|s| !s.is_empty()) else {
        return default_width;
    };

    // Split on top-level commas (outside parentheses).
    let mut entries = Vec::new();
    let mut start = 0usize;
    let mut depth = 0usize;
    for (idx, ch) in raw.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                entries.push(raw[start..idx].trim());
                start = idx + 1;
            }
            _ => {}
        }
    }
    entries.push(raw[start..].trim());

    for entry in entries {
        if entry.is_empty() {
            continue;
        }
        // Separate optional `<media-condition>` from trailing `<source-size-value>` at the
        // last top-level ASCII whitespace outside parentheses.
        let mut split_at = None;
        let mut p_depth = 0usize;
        for (idx, ch) in entry.char_indices() {
            match ch {
                '(' => p_depth += 1,
                ')' => p_depth = p_depth.saturating_sub(1),
                c if c.is_ascii_whitespace() && p_depth == 0 => {
                    split_at = Some(idx);
                }
                _ => {}
            }
        }
        let (cond_opt, len_str) = match split_at {
            Some(idx) => {
                let cond = entry[..idx].trim();
                let len = entry[idx..].trim();
                (if cond.is_empty() { None } else { Some(cond) }, len)
            }
            None => (None, entry),
        };
        let Some(px) = parse_size_length_px(len_str, media) else {
            continue;
        };
        if let Some(cond) = cond_opt {
            let list = megi_css::media::parse_media_query_list(cond);
            if !megi_css::media::evaluate(&list, media) {
                continue;
            }
        }
        return px;
    }

    default_width
}

/// Returns whether a `<source type="...">` MIME type is decodable by the engine.
pub fn is_supported_image_mime(mime: &str) -> bool {
    let essential = mime
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if essential.is_empty() {
        return true;
    }
    matches!(
        essential.as_str(),
        "image/png" | "image/jpeg" | "image/jpg" | "image/gif" | "image/webp" | "image/svg+xml"
    )
}

/// Selects the best image `(url, effective_density)` from `srcset` / `src` / `sizes`.
pub fn select_image_candidate(
    srcset: Option<&str>,
    fallback_src: Option<&str>,
    sizes_attr: Option<&str>,
    media: &megi_css::MediaContext,
    dpr: f32,
) -> Option<(String, f32)> {
    let dpr = sanitize_dpr(dpr);
    let mut candidates = srcset.map(parse_srcset).unwrap_or_default();

    let fallback = fallback_src.map(str::trim).filter(|s| !s.is_empty());
    let uses_width = candidates
        .iter()
        .any(|c| matches!(c.descriptor, SrcsetDescriptor::Width(_)));

    // When using density descriptors (or empty srcset), include `<img src>` as `1x`
    // unless `srcset` already defines a `1x` candidate (WHATWG HTML §4.8.4.3.5).
    if !uses_width
        && let Some(src) = fallback
        && !candidates
            .iter()
            .any(|c| matches!(c.descriptor, SrcsetDescriptor::Density(d) if (d - 1.0).abs() < 1e-4))
    {
        candidates.push(SrcsetCandidate {
            url: src.to_string(),
            descriptor: SrcsetDescriptor::Density(1.0),
        });
    }

    if candidates.is_empty() {
        return fallback.map(|s| (s.to_string(), 1.0));
    }

    let target_width = if uses_width {
        resolve_sizes(sizes_attr, media).max(1.0)
    } else {
        1.0
    };

    let mut weighted: Vec<(String, f32)> = candidates
        .into_iter()
        .map(|c| {
            let density = match c.descriptor {
                SrcsetDescriptor::Density(d) => d,
                SrcsetDescriptor::Width(w) => (w as f32 / target_width).max(0.01),
            };
            (c.url, density)
        })
        .collect();

    // Sort ascending by effective density (stable tie-breaker preserves source order).
    weighted.sort_by(|a, b| a.1.total_cmp(&b.1));

    // Pick the smallest density >= dpr, or fall back to the highest density available.
    if let Some(best) = weighted.iter().find(|(_, d)| *d + 1e-4 >= dpr) {
        Some(best.clone())
    } else {
        weighted.last().cloned()
    }
}
