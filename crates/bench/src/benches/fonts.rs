//! Font shaping/measure workloads (feature-gated).

use super::*;

/// Optional font benchmarks (`--features fonts`).
///
/// They load the DejaVu fixture that ships with `crates/fonts`; when the
/// fixture is absent the benchmarks are reported as skipped instead of failing
/// the suite (the `fonts` crate is still being migrated during the sprint).
pub(super) fn register_fonts(suite: &mut Suite, scale: f64) {
    use megi_fonts::{FontDatabase, FontStyle};

    let root = crate::runner::workspace_root();
    let candidates = [
        root.join("crates/fonts/assets/DejaVuSans.ttf"),
        root.join("crates/fonts/tests/fixtures/DejaVuSans.ttf"),
    ];
    let Some(path) = candidates.iter().find(|path| path.is_file()) else {
        suite.skip(
            "fonts/measure-1k-words",
            "DejaVuSans.ttf no encontrado (fixture de crates/fonts ausente)",
        );
        suite.skip("fonts/shape-1k-words", "DejaVuSans.ttf no encontrado");
        return;
    };
    let Ok(bytes) = std::fs::read(path) else {
        suite.skip("fonts/measure-1k-words", "no se pudo leer DejaVuSans.ttf");
        suite.skip("fonts/shape-1k-words", "no se pudo leer DejaVuSans.ttf");
        return;
    };
    let mut database = FontDatabase::new();
    if let Err(error) = database.load_font_data(bytes) {
        let reason = format!("font fixture inválido: {error}");
        suite.skip("fonts/measure-1k-words", reason.clone());
        suite.skip("fonts/shape-1k-words", reason);
        return;
    }
    let Ok(measure_font) = database.font(
        &["DejaVu Sans"],
        400,
        FontStyle::Normal,
        fontdb::Stretch::Normal,
    ) else {
        let reason = "DejaVu Sans no resuelve".to_string();
        suite.skip("fonts/measure-1k-words", reason.clone());
        suite.skip("fonts/shape-1k-words", reason);
        return;
    };
    let Ok(shape_font) = database.font(
        &["DejaVu Sans"],
        400,
        FontStyle::Normal,
        fontdb::Stretch::Normal,
    ) else {
        let reason = "DejaVu Sans no resuelve".to_string();
        suite.skip("fonts/measure-1k-words", reason.clone());
        suite.skip("fonts/shape-1k-words", reason);
        return;
    };
    let words = generators::words(generators::scale(generators::FONT_WORDS, scale));
    let word_count = words.len();
    let measure_words = words.clone();
    suite.add(
        BenchEntry::new(
            "fonts/measure-1k-words",
            "fonts",
            format!("{word_count} palabras, DejaVu Sans 16px"),
            move || {
                let mut total = 0.0f32;
                for word in &measure_words {
                    if let Ok(width) = measure_font.measure_text(16.0, word) {
                        total += width;
                    }
                }
                black_box(total);
            },
        )
        .with_units(word_count as f64, "words"),
    );
    suite.add(
        BenchEntry::new(
            "fonts/shape-1k-words",
            "fonts",
            format!("{word_count} palabras, DejaVu Sans 16px"),
            move || {
                let mut glyphs = 0usize;
                for word in &words {
                    if let Ok(run) = shape_font.shape(16.0, word) {
                        glyphs += run.glyphs.len();
                    }
                }
                black_box(glyphs);
            },
        )
        .with_units(word_count as f64, "words"),
    );
}
