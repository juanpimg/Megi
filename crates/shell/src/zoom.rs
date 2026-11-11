//! Page zoom: deterministic zoom levels, clamping and CSS viewport math.
//!
//! The renderer works in **CSS pixels**; the window buffer is in **physical pixels**.
//! At device scale `s` and page zoom `z` the CSS viewport is
//! `physical / (s * z)`, so `--zoom 2` shows half as many CSS pixels (everything twice
//! as large) independently of the HiDPI scale factor. The presentation path upscales the
//! CSS frame to the window buffer, so the zoom factor needs no separate blit change.
//!
//! Controls: `Ctrl+=`/`Ctrl++` zoom in, `Ctrl+-` zoom out, `Ctrl+0` reset to 100 %
//! (plain `=`/`+`/`-`/`0` work too). The factor is clamped to
//! [`MIN_ZOOM`]..=[`MAX_ZOOM`] and the levels are the fixed ladder in [`ZOOM_LEVELS`]
//! (deterministic steps, no accumulation drift).

/// Smallest accepted zoom factor (50 %).
pub const MIN_ZOOM: f64 = 0.5;
/// Largest accepted zoom factor (300 %).
pub const MAX_ZOOM: f64 = 3.0;
/// Default zoom factor (100 %).
pub const DEFAULT_ZOOM: f64 = 1.0;
/// Fixed zoom ladder used by the keyboard shortcuts and `--zoom`.
pub const ZOOM_LEVELS: [f64; 8] = [0.5, 0.75, 1.0, 1.25, 1.5, 2.0, 2.5, 3.0];

/// Sanitizes a zoom factor: non-finite/non-positive values fall back to
/// [`DEFAULT_ZOOM`], everything else is clamped to `MIN_ZOOM..=MAX_ZOOM`.
pub fn sanitize_zoom(zoom: f64) -> f64 {
    if !zoom.is_finite() || zoom <= 0.0 {
        DEFAULT_ZOOM
    } else {
        zoom.clamp(MIN_ZOOM, MAX_ZOOM)
    }
}

/// Next zoom level above `current` (stops at [`MAX_ZOOM`]).
pub fn zoom_in(current: f64) -> f64 {
    let current = sanitize_zoom(current);
    ZOOM_LEVELS
        .iter()
        .copied()
        .find(|level| *level > current + 1e-9)
        .unwrap_or(MAX_ZOOM)
}

/// Previous zoom level below `current` (stops at [`MIN_ZOOM`]).
pub fn zoom_out(current: f64) -> f64 {
    let current = sanitize_zoom(current);
    ZOOM_LEVELS
        .iter()
        .rev()
        .copied()
        .find(|level| *level < current - 1e-9)
        .unwrap_or(MIN_ZOOM)
}

/// Resets the zoom factor to 100 %.
pub const fn zoom_reset() -> f64 {
    DEFAULT_ZOOM
}

/// Formats the zoom factor for the window title (`100%`, `125%`, `300%`).
pub fn zoom_label(zoom: f64) -> String {
    format!("{}%", (sanitize_zoom(zoom) * 100.0).round() as u32)
}

/// Returns `true` when `zoom` is a value the CLI accepts verbatim
/// (`0.5..=3.0`, finite). Keyboard adjustments clamp instead of failing.
pub fn is_valid_cli_zoom(zoom: f64) -> bool {
    zoom.is_finite() && (MIN_ZOOM..=MAX_ZOOM).contains(&zoom)
}

/// Converts a physical window size into the CSS render viewport for `zoom`.
///
/// `device_scale` is the HiDPI scale factor (`physical / CSS`); the effective scale is
/// `device_scale * zoom`. The result is at least 1 px and clamped to the render cap.
pub fn zoomed_viewport(
    physical_width: u32,
    physical_height: u32,
    device_scale: f64,
    zoom: f64,
) -> (u32, u32) {
    let device_scale = crate::scale::sanitize_scale(device_scale);
    let zoom = sanitize_zoom(zoom);
    let effective = device_scale * zoom;
    let width = ((f64::from(physical_width.max(1)) / effective).round() as u32)
        .clamp(1, render::MAX_VIEWPORT_DIMENSION);
    let height = ((f64::from(physical_height.max(1)) / effective).round() as u32)
        .clamp(1, render::MAX_VIEWPORT_DIMENSION);
    (width, height)
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn sanitize_clamps_and_recovers_invalid_values() {
        assert_eq!(sanitize_zoom(1.0), 1.0);
        assert_eq!(sanitize_zoom(0.1), MIN_ZOOM);
        assert_eq!(sanitize_zoom(10.0), MAX_ZOOM);
        assert_eq!(sanitize_zoom(f64::NAN), DEFAULT_ZOOM);
        assert_eq!(sanitize_zoom(0.0), DEFAULT_ZOOM);
        assert_eq!(sanitize_zoom(-2.0), DEFAULT_ZOOM);
    }

    #[test]
    fn zoom_steps_follow_the_fixed_ladder() {
        assert_eq!(zoom_in(1.0), 1.25);
        assert_eq!(zoom_in(1.25), 1.5);
        assert_eq!(zoom_out(1.0), 0.75);
        assert_eq!(zoom_out(0.75), 0.5);
        assert_eq!(zoom_in(2.9), 3.0);
        assert_eq!(zoom_out(0.6), 0.5);
        assert_eq!(zoom_in(3.0), MAX_ZOOM);
        assert_eq!(zoom_out(0.5), MIN_ZOOM);
    }

    #[test]
    fn non_ladder_values_step_to_the_next_level() {
        // A `--zoom 1.1` starts from a non-ladder value but the keyboard still walks the
        // deterministic ladder (no drift from repeated multiplication).
        assert_eq!(zoom_in(1.1), 1.25);
        assert_eq!(zoom_out(1.1), 1.0);
    }

    #[test]
    fn reset_and_label() {
        assert_eq!(zoom_reset(), 1.0);
        assert_eq!(zoom_label(1.0), "100%");
        assert_eq!(zoom_label(0.75), "75%");
        assert_eq!(zoom_label(3.0), "300%");
        assert_eq!(zoom_label(1.234), "123%");
    }

    #[test]
    fn cli_zoom_validation() {
        assert!(is_valid_cli_zoom(0.5));
        assert!(is_valid_cli_zoom(1.0));
        assert!(is_valid_cli_zoom(3.0));
        assert!(!is_valid_cli_zoom(0.49));
        assert!(!is_valid_cli_zoom(3.01));
        assert!(!is_valid_cli_zoom(f64::NAN));
        assert!(!is_valid_cli_zoom(f64::INFINITY));
    }

    #[test]
    fn viewport_is_physical_over_device_scale_times_zoom() {
        assert_eq!(zoomed_viewport(800, 600, 1.0, 1.0), (800, 600));
        assert_eq!(zoomed_viewport(800, 600, 1.0, 2.0), (400, 300));
        assert_eq!(zoomed_viewport(800, 600, 2.0, 1.0), (400, 300));
        // HiDPI and zoom compose: 800 physical at scale 2 and zoom 2 = 200 CSS px.
        assert_eq!(zoomed_viewport(800, 600, 2.0, 2.0), (200, 150));
        // Never returns a zero viewport, even with absurd factors.
        assert_eq!(zoomed_viewport(0, 0, 1.0, 3.0), (1, 1));
    }
}

/// Maximum number of persisted per-origin zoom entries.
pub const MAX_SITE_ZOOMS: usize = 64;

/// Returns the origin key used for per-site zoom (`scheme://host[:port]` for http(s), and
/// the scheme for `file:`); `None` for ephemeral/data URLs.
pub fn site_origin(url: &str) -> Option<String> {
    let parsed = net::Url::parse(url).ok()?;
    match parsed.scheme() {
        net::Scheme::Http | net::Scheme::Https => {
            let host = parsed.host();
            if host.is_empty() {
                None
            } else {
                Some(format!("{}://{}", parsed.scheme().as_str(), host))
            }
        }
        net::Scheme::File => Some("file://".to_string()),
        _ => None,
    }
}

/// Looks up a persisted zoom for `url`'s origin (newest entry wins).
pub fn site_zoom(entries: &[(String, f64)], url: &str) -> Option<f64> {
    let origin = site_origin(url)?;
    entries
        .iter()
        .find(|(key, _)| *key == origin)
        .map(|(_, zoom)| sanitize_zoom(*zoom))
}

/// Records/updates the zoom for `url`'s origin (newest first, capped, deduplicated).
pub fn record_site_zoom(entries: &mut Vec<(String, f64)>, url: &str, zoom: f64) {
    let Some(origin) = site_origin(url) else {
        return;
    };
    let zoom = sanitize_zoom(zoom);
    entries.retain(|(key, _)| *key != origin);
    entries.insert(0, (origin, zoom));
    entries.truncate(MAX_SITE_ZOOMS);
}

/// Tolerant normalization of persisted site zooms: valid origins, sanitized zoom values,
/// newest-first order preserved, duplicates collapsed, capped at [`MAX_SITE_ZOOMS`].
pub fn normalize_site_zooms(entries: Vec<(String, f64)>) -> Vec<(String, f64)> {
    let mut out: Vec<(String, f64)> = Vec::new();
    for (origin, zoom) in entries {
        let origin = origin.trim().to_lowercase();
        if origin.is_empty() || out.iter().any(|(key, _)| *key == origin) {
            continue;
        }
        out.push((origin, sanitize_zoom(zoom)));
        if out.len() == MAX_SITE_ZOOMS {
            break;
        }
    }
    out
}

#[cfg(test)]
mod site_zoom_tests {
    use super::*;

    #[test]
    fn site_origin_normalizes_http_and_file() {
        assert_eq!(
            site_origin("https://example.com/a?x=1").as_deref(),
            Some("https://example.com")
        );
        assert_eq!(
            site_origin("http://example.com:8080/").as_deref(),
            Some("http://example.com")
        );
        assert_eq!(
            site_origin("file:///tmp/a.html").as_deref(),
            Some("file://")
        );
        assert!(site_origin("data:text/html,x").is_none());
    }

    #[test]
    fn record_apply_and_normalize_site_zooms() {
        let mut entries = Vec::new();
        record_site_zoom(&mut entries, "https://a.example/x", 1.5);
        record_site_zoom(&mut entries, "https://b.example/y", 0.75);
        record_site_zoom(&mut entries, "https://a.example/z", 2.0);
        assert_eq!(entries.len(), 2);
        assert_eq!(site_zoom(&entries, "https://a.example/other"), Some(2.0));
        assert_eq!(site_zoom(&entries, "https://c.example/"), None);
        let normalized = normalize_site_zooms(entries);
        assert_eq!(normalized.len(), 2);
        assert!(
            normalized
                .iter()
                .all(|(_, zoom)| (0.5..=3.0).contains(zoom))
        );
    }

    #[test]
    fn site_zoom_cap_is_enforced() {
        let mut entries = Vec::new();
        for index in 0..(MAX_SITE_ZOOMS + 10) {
            record_site_zoom(
                &mut entries,
                &format!("https://host{index}.example/"),
                1.0 + index as f64 / 100.0,
            );
        }
        assert_eq!(entries.len(), MAX_SITE_ZOOMS);
        assert_eq!(normalize_site_zooms(entries).len(), MAX_SITE_ZOOMS);
    }
}
