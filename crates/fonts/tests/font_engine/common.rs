//! Bundled font fixtures shared by the font-engine integration tests.

pub(crate) use megi_fonts::{FontDatabase, FontError, FontStyle};

pub(crate) const DEJAVU_SANS: &[u8] = include_bytes!("../../assets/DejaVuSans.ttf");
pub(crate) const DEJAVU_SANS_BOLD: &[u8] = include_bytes!("../../assets/DejaVuSans-Bold.ttf");

pub(crate) fn fixture_db() -> FontDatabase {
    let mut db = FontDatabase::new();
    db.load_font_data(DEJAVU_SANS.to_vec())
        .expect("bundled DejaVuSans.ttf must be a valid font");
    db.load_font_data(DEJAVU_SANS_BOLD.to_vec())
        .expect("bundled DejaVuSans-Bold.ttf must be a valid font");
    db
}
