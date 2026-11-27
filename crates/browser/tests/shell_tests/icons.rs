//! SVG icon atlas and semantic chrome states (WP-USER-ui-icons).

use megi_browser::chrome::tokens::ShellPalette;
use megi_browser::chrome::{Icon, IconAtlas, STATE};
use megi_webview::layout::Rect;
use megi_webview::paint::{Color, Pixmap};

/// Relative luminance per WCAG 2.2 §1.4.3 (sRGB linearization).
fn luminance(color: Color) -> f64 {
    let linear = |channel: u8| -> f64 {
        let value = f64::from(channel) / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
}

fn contrast(foreground: Color, background: Color) -> f64 {
    let lighter = luminance(foreground).max(luminance(background));
    let darker = luminance(foreground).min(luminance(background));
    (lighter + 0.05) / (darker + 0.05)
}

#[test]
fn icon_atlas_rasterizes_every_shell_icon() {
    let atlas = IconAtlas::new();
    assert_eq!(Icon::ALL.len(), 14, "the shell has fourteen semantic icons");
    for icon in Icon::ALL {
        let mut pixmap = Pixmap::new(20, 20);
        let painted = atlas.paint(
            &mut pixmap,
            *icon,
            Rect::new(0.0, 0.0, 20.0, 20.0),
            Color::WHITE,
            1.0,
        );
        assert!(painted > 0, "icon {icon:?} produced no pixels");
        assert!(
            pixmap.pixels.iter().any(|pixel| pixel.a > 0),
            "icon {icon:?} left the pixmap transparent"
        );
    }
}

#[test]
fn icon_paint_applies_requested_color() {
    let atlas = IconAtlas::new();
    let tint = Color::new(0x12, 0x34, 0x56, 0xFF);
    let mut pixmap = Pixmap::new(20, 20);
    let painted = atlas.paint(
        &mut pixmap,
        Icon::Gear,
        Rect::new(0.0, 0.0, 20.0, 20.0),
        tint,
        1.0,
    );
    assert!(painted > 0, "gear icon produced no pixels");
    let visible: Vec<Color> = pixmap
        .pixels
        .iter()
        .copied()
        .filter(|pixel| pixel.a > 0)
        .collect();
    assert!(!visible.is_empty(), "expected tinted pixels");
    assert!(
        visible
            .iter()
            .all(|pixel| pixel.r == tint.r && pixel.g == tint.g && pixel.b == tint.b),
        "source-over tint must use the requested color"
    );
}

#[test]
fn oversized_icon_is_rejected() {
    let bytes = vec![b'x'; 1_000_000];
    assert!(megi_webview::render::decode_image_with_svg(&bytes, 16).is_none());
}

#[test]
fn state_palette_focus_contrast_is_at_least_4_5() {
    let background = ShellPalette::DARK.top;
    let ratio = contrast(STATE.focus, background);
    assert!(
        ratio >= 4.5,
        "focus contrast {ratio:.2}:1 over shell top must clear WCAG 2.2 §1.4.3"
    );
}
