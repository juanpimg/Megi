//! Prints the measured metrics of the deterministic test font (DejaVu Sans) at
//! 16 px, so layout/paint integrators can sanity-check their numbers.
//!
//! Run with `cargo run -p fonts --example report`.

use fonts::{FontDatabase, FontStyle};

const DEJAVU_SANS: &[u8] = include_bytes!("../tests/fixtures/DejaVuSans.ttf");
const DEJAVU_SANS_BOLD: &[u8] = include_bytes!("../tests/fixtures/DejaVuSans-Bold.ttf");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut db = FontDatabase::new();
    db.load_font_data(DEJAVU_SANS.to_vec())?;
    db.load_font_data(DEJAVU_SANS_BOLD.to_vec())?;
    println!("faces loaded: {} -> {:?}", db.len(), db.family_names());

    let regular = db.font(&["DejaVu Sans"], 400, FontStyle::Normal)?;
    let bold = db.font(&["DejaVu Sans"], 700, FontStyle::Normal)?;
    println!(
        "regular: {} ({}), upem={}",
        regular.info().post_script_name,
        regular.family_name(),
        regular.units_per_em()
    );
    println!("bold:    {}", bold.info().post_script_name);

    let metrics = regular.metrics(16.0)?;
    println!("metrics @16px: {metrics:?}");
    println!(
        "line_height={:.3} baseline_from_top={:.3}",
        metrics.line_height(),
        metrics.baseline_from_top()
    );

    let run = regular.shape(16.0, "Hello fi")?;
    println!(
        "shape(\"Hello fi\") width={:.3}px glyphs={} fonts={:?}",
        run.width,
        run.glyphs.len(),
        run.fonts
            .iter()
            .map(|f| &f.post_script_name)
            .collect::<Vec<_>>()
    );
    println!(
        "measure_text unshaped = {:.3}px",
        db.measure_text(&["sans-serif"], 400, FontStyle::Normal, 16.0, "Hello fi")?
    );

    let glyph_id = regular.glyph_id('H').ok_or("H has no glyph")?;
    let mask = db.rasterize_glyph(&["DejaVu Sans"], 400, FontStyle::Normal, 16.0, glyph_id)?;
    let covered = mask.data.iter().filter(|&&alpha| alpha > 0).count();
    println!(
        "raster 'H' @16px: {}x{} left={:.2} top={:.2} advance={:.3} covered_px={}",
        mask.width, mask.height, mask.left, mask.top, mask.advance, covered
    );
    Ok(())
}
