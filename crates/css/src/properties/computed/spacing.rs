//! Computed-value resolution for `word-spacing` and `letter-spacing`.
//!
//! Pure helper extracted from the typography chunk so that chunk stays under
//! the architecture size cap. Computed spacing lengths are absolute: a
//! percentage resolves against the element's own font size (CSS Text 3 §8.4).

use super::*;

pub(super) fn inherited(
    entries: &[(&str, &Declaration)],
    property: &str,
    font_size: f32,
    root_font_size: f32,
    parent_value: Option<Length>,
) -> Length {
    let Some(decl) = find_declaration(entries, property) else {
        return parent_value.unwrap_or(Length::Px(0.0));
    };
    let value = decl.raw_value.trim();
    if value.eq_ignore_ascii_case("normal") {
        return Length::Px(0.0);
    }
    match Length::parse(value, font_size, root_font_size) {
        Some(Length::Percent(pct)) => Length::Px(font_size * pct / 100.0),
        Some(length) => length,
        None => parent_value.unwrap_or(Length::Px(0.0)),
    }
}
