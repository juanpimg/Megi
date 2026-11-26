//! Regresión visual del caret gris del `<select>` en rust-lang.org a 1024×768:
//! el triángulo `::after` aparecía como una franja fuera del borde superior
//! derecho del control. El test es observable: lee la display list real del
//! pipeline HTML → CSS → layout → paint.
//!
//! Nota: el envolvente de «Blog» ya está cubierto por
//! `crates/layout/tests/flex/nav_wrap.rs` en `origin/main`; aquí solo se cubre
//! el caret, que es la parte no solapada de este work package.
//!
//! CSS 2.2 §9.6/§10.3.7 (posicionamiento absoluto y altura usada).

use std::path::PathBuf;

use megi_display::DisplayItem;
use megi_layout::{BoxType, LayoutBox};
use megi_render::{RenderOptions, Viewport, render_url};

fn render_fixture() -> megi_render::RenderOutput {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .join("tests/fixtures/render/rust_nav.html");
    let url = format!(
        "file://{}",
        fixture.display().to_string().replace(' ', "%20")
    );
    render_url(
        &url,
        &RenderOptions {
            viewport: Viewport::new(1024, 768).expect("viewport"),
            ..RenderOptions::default()
        },
    )
    .expect("render rust_nav fixture")
}

fn replaced_box(root: &LayoutBox) -> Option<&LayoutBox> {
    if matches!(root.box_type, BoxType::Replaced(_)) {
        return Some(root);
    }
    root.children.iter().find_map(replaced_box)
}

/// Cada scanline del caret (tanto el nativo `SolidColor` como el `Border` del
/// `::after` del autor) debe caer dentro del border box del `<select>`, nunca
/// por encima del borde (la franja gris observada en `after.png`).
#[test]
fn select_caret_stays_inside_the_border_box() {
    let output = render_fixture();
    let select = replaced_box(&output.layout).expect("select replaced box");
    let rect = select.dimensions.border_box();
    let caret: Vec<_> = output
        .display_list
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::SolidColor { rect: r, color }
                if r.width > 0.0
                    && r.width <= 12.0
                    && r.height > 0.0
                    && r.height <= 2.0
                    && color.r < 128
                    && color.a > 0 =>
            {
                Some(*r)
            }
            DisplayItem::Border {
                rect: r,
                sides: [top, right, _, left],
            } if r.x + r.width > rect.x
                && r.x < rect.x + rect.width
                && r.y + r.height > rect.y - 20.0
                && r.y < rect.y
                && top.color.r < 128
                && right.color.a == 0
                && left.color.a == 0 =>
            {
                // `.select::after`: triángulo CSS posicionado sobre el borde.
                Some(*r)
            }
            _ => None,
        })
        .collect();
    assert!(
        caret.len() >= 3,
        "expected >=3 caret scanlines, got {caret:?}"
    );
    for scanline in caret {
        assert!(
            scanline.x >= rect.x
                && scanline.x + scanline.width <= rect.x + rect.width
                && scanline.y >= rect.y
                && scanline.y + scanline.height <= rect.y + rect.height,
            "caret scanline {:?} escaped select border box {:?}",
            scanline,
            rect
        );
    }
}

/// Golden generado por el propio pipeline (`render` bin) tras el fix del caret.
/// Sin el fix, el `::after` queda fuera del borde (rect y=85.6 vs borde y=91.7).
/// md5 golden: `a15a1dde5b1755d3b9478e629b114285`
/// (el fixture es idéntico al de `origin/main`; el PNG cambió solo por el caret).
#[test]
fn fixture_png_is_byte_identical_to_golden() {
    let out = render_fixture();
    let golden = include_bytes!("golden/rust_nav.png");
    assert_eq!(
        out.png.len(),
        golden.len(),
        "rust_nav PNG length changed; regenerate golden/rust_nav.png"
    );
    assert!(out.png == golden, "rust_nav PNG drifted from golden");
}
