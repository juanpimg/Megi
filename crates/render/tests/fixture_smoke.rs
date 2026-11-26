//! #1431: every render HTML fixture is executed at least once by a test.
//!
//! Dedicated suites assert the interesting pixels of individual fixtures; this smoke suite
//! renders all of them so a fixture that stops parsing/painting cannot silently rot in the
//! tree while the docs still present it as covered.

use std::path::PathBuf;

use megi_render::{RenderOptions, Viewport, render_url};

/// Versioned inventory of the render fixtures this smoke suite promises to run (#2436).
const FIXTURE_INDEX: &str = include_str!("../../../tests/fixtures/render/FIXTURE-INDEX.txt");

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .join("tests/fixtures/render")
}

#[test]
fn every_render_html_fixture_renders_without_fatal_error() {
    let mut dir = std::fs::read_dir(fixtures_dir()).expect("fixtures dir");
    let mut names: Vec<String> = dir
        .by_ref()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "html"))
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    // #2436: `>= 25` let fixtures disappear from the smoke suite unnoticed. The
    // versioned index is the exact no-break promise: 126 HTML fixtures today.
    let expected: Vec<&str> = FIXTURE_INDEX
        .lines()
        .filter(|line| !line.is_empty())
        .collect();
    let actual: Vec<&str> = names.iter().map(String::as_str).collect();
    assert_eq!(
        actual, expected,
        "render fixture inventory changed: update FIXTURE-INDEX.txt with the new set"
    );

    let options = RenderOptions {
        viewport: Viewport::new(320, 240).expect("viewport"),
        ..RenderOptions::default()
    };
    for name in &names {
        let url = format!(
            "file://{}",
            fixtures_dir()
                .join(name)
                .display()
                .to_string()
                .replace(' ', "%20")
        );
        let output = render_url(&url, &options)
            .unwrap_or_else(|error| panic!("render fixture {name}: {error}"));
        assert!(
            !output.pixmap.pixels.is_empty(),
            "fixture {name} must produce a non-empty pixmap"
        );
        assert!(
            output.pixmap.pixels.iter().any(|pixel| pixel.a != 0),
            "fixture {name} must paint at least one opaque pixel"
        );
        assert!(
            output.pixmap.width > 0 && output.pixmap.height > 0,
            "fixture {name} must have a sized pixmap"
        );
    }
}
