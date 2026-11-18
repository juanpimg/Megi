pub(crate) use megi_css::{BorderStyle, Color};
pub(crate) use megi_layout::Rect;
pub(crate) use megi_paint::{
    BorderSide, DisplayItem, DisplayList, Pixmap, RenderConfig,
    rasterize_display_list_with_font_database, rasterize_display_list_with_font_database_and_stats,
};

/// Bundled font database used by the reference pipeline.
pub(crate) fn database() -> &'static megi_fonts::FontDatabase {
    megi_paint::bundled_font_database()
}

/// A red border side of the given width.
pub(crate) fn side(width: f32, style: BorderStyle, color: Color) -> BorderSide {
    BorderSide::new(width, style, color)
}

/// Counts how much red ink a white-canvas pixmap contains: `sum(255 - g)` over
/// all pixels equals `255 * covered_area` because a pure-red source at alpha
/// `a` leaves `g = 255 * (1 - a)` on white.
pub(crate) fn red_ink_mass(pixmap: &Pixmap) -> f64 {
    pixmap
        .pixels
        .iter()
        .map(|pixel| f64::from(255 - pixel.g))
        .sum::<f64>()
        / 255.0
}
