//! Embedded SVG atlas for shell affordances (WP-USER-ui-icons).
//!
//! Each icon is a small, hand-authored SVG rasterized once at construction with
//! [`megi_webview::render::decode_image_with_svg`]. [`IconAtlas::paint`] treats the
//! raster as an alpha mask and composites the caller's color with source-over, so
//! one atlas serves every [`StatePalette`](super::tokens::StatePalette) state.

use megi_webview::image::RgbaImage;
use megi_webview::layout::Rect;
use megi_webview::paint::{Color, Pixmap};

/// Intrinsic raster size, in pixels, of every embedded glyph.
const ICON_SIZE: u32 = 20;
/// Hard cap per embedded document, matching the shell favicon budget.
const MAX_SVG_BYTES: usize = 64 * 1024;

const BACK: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24"><path d="M15 18 9 12l6-6" fill="none" stroke="#fff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>"##;
const FORWARD: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24"><path d="m9 18 6-6-6-6" fill="none" stroke="#fff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>"##;
const RELOAD: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24"><path d="M20 12a8 8 0 1 1-2.34-5.66M20 4v8h-8" fill="none" stroke="#fff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>"##;
const TOGGLE_SIDEBAR: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24"><rect x="3" y="4" width="18" height="16" rx="2" fill="none" stroke="#fff" stroke-width="2"/><path d="M9 4v16" fill="none" stroke="#fff" stroke-width="2"/></svg>"##;
const GEAR: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24"><circle cx="12" cy="12" r="3" fill="none" stroke="#fff" stroke-width="2"/><path d="M12 2v3M12 19v3M2 12h3M19 12h3M4.9 4.9 7 7M17 17l2.1 2.1M19.1 4.9 17 7M7 17l-2.1 2.1" fill="none" stroke="#fff" stroke-width="2" stroke-linecap="round"/></svg>"##;
const SHARE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24"><circle cx="18" cy="5" r="3" fill="none" stroke="#fff" stroke-width="2"/><circle cx="6" cy="12" r="3" fill="none" stroke="#fff" stroke-width="2"/><circle cx="18" cy="19" r="3" fill="none" stroke="#fff" stroke-width="2"/><path d="m8.6 10.5 6.8-4M8.6 13.5l6.8 4" fill="none" stroke="#fff" stroke-width="2"/></svg>"##;
const PLUS: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24"><path d="M12 5v14M5 12h14" fill="none" stroke="#fff" stroke-width="2" stroke-linecap="round"/></svg>"##;
const MENU: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24"><path d="M4 7h16M4 12h16M4 17h16" fill="none" stroke="#fff" stroke-width="2" stroke-linecap="round"/></svg>"##;
const SEARCH: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24"><circle cx="11" cy="11" r="7" fill="none" stroke="#fff" stroke-width="2"/><path d="m20 20-4-4" fill="none" stroke="#fff" stroke-width="2" stroke-linecap="round"/></svg>"##;
const GLOBE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24"><circle cx="12" cy="12" r="9" fill="none" stroke="#fff" stroke-width="2"/><path d="M3 12h18M12 3c3 3 3 15 0 18M12 3c-3 3-3 15 0 18" fill="none" stroke="#fff" stroke-width="2"/></svg>"##;
const FOLDER: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24"><path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" fill="none" stroke="#fff" stroke-width="2" stroke-linejoin="round"/></svg>"##;
const PEOPLE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24"><circle cx="9" cy="8" r="3" fill="none" stroke="#fff" stroke-width="2"/><path d="M3 20v-1a6 6 0 0 1 12 0v1M16 5a3 3 0 0 1 0 6M17 14a6 6 0 0 1 4 6" fill="none" stroke="#fff" stroke-width="2" stroke-linecap="round"/></svg>"##;
const HELP: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24"><circle cx="12" cy="12" r="9" fill="none" stroke="#fff" stroke-width="2"/><path d="M9.5 9a2.5 2.5 0 1 1 4 2c-.9.7-1.5 1.2-1.5 2.5" fill="none" stroke="#fff" stroke-width="2" stroke-linecap="round"/><circle cx="12" cy="17" r="1" fill="#fff"/></svg>"##;
const USER: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24"><circle cx="12" cy="8" r="4" fill="none" stroke="#fff" stroke-width="2"/><path d="M4 21a8 8 0 0 1 16 0" fill="none" stroke="#fff" stroke-width="2" stroke-linecap="round"/></svg>"##;

/// Semantic shell glyphs. The order is part of the public atlas contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    /// Navigate to the previous history entry.
    Back,
    /// Navigate to the next history entry.
    Forward,
    /// Reload the current document.
    Reload,
    /// Show or hide the sidebar.
    ToggleSidebar,
    /// Open browser settings.
    Gear,
    /// Share the current page.
    Share,
    /// Create a new tab.
    Plus,
    /// Open the main menu.
    Menu,
    /// Focus the search field.
    Search,
    /// Represent a web origin.
    Globe,
    /// Represent a folder or file location.
    Folder,
    /// Represent contacts or profiles.
    People,
    /// Open help content.
    Help,
    /// Represent the current user.
    User,
}

impl Icon {
    /// Every shell icon, in the order `Icon::ALL` promises to callers.
    pub const ALL: &'static [Self] = &[
        Self::Back,
        Self::Forward,
        Self::Reload,
        Self::ToggleSidebar,
        Self::Gear,
        Self::Share,
        Self::Plus,
        Self::Menu,
        Self::Search,
        Self::Globe,
        Self::Folder,
        Self::People,
        Self::Help,
        Self::User,
    ];

    /// Position of this icon in the rasterized mask table.
    const fn index(self) -> usize {
        match self {
            Self::Back => 0,
            Self::Forward => 1,
            Self::Reload => 2,
            Self::ToggleSidebar => 3,
            Self::Gear => 4,
            Self::Share => 5,
            Self::Plus => 6,
            Self::Menu => 7,
            Self::Search => 8,
            Self::Globe => 9,
            Self::Folder => 10,
            Self::People => 11,
            Self::Help => 12,
            Self::User => 13,
        }
    }

    /// The embedded document for this glyph.
    const fn svg(self) -> &'static str {
        match self {
            Self::Back => BACK,
            Self::Forward => FORWARD,
            Self::Reload => RELOAD,
            Self::ToggleSidebar => TOGGLE_SIDEBAR,
            Self::Gear => GEAR,
            Self::Share => SHARE,
            Self::Plus => PLUS,
            Self::Menu => MENU,
            Self::Search => SEARCH,
            Self::Globe => GLOBE,
            Self::Folder => FOLDER,
            Self::People => PEOPLE,
            Self::Help => HELP,
            Self::User => USER,
        }
    }
}

/// Pre-rasterized alpha masks for every [`Icon`].
///
/// Construction is fallible internally: a glyph that fails to decode is simply
/// absent and [`IconAtlas::paint`] writes nothing for it.
#[derive(Debug)]
pub struct IconAtlas {
    masks: Vec<Option<RgbaImage>>,
}

impl Default for IconAtlas {
    fn default() -> Self {
        Self::new()
    }
}

impl IconAtlas {
    /// Rasterizes all embedded SVGs at [`ICON_SIZE`] pixels each.
    pub fn new() -> Self {
        let masks = Icon::ALL
            .iter()
            .map(|icon| {
                let svg = icon.svg();
                if svg.len() > MAX_SVG_BYTES {
                    return None;
                }
                let image = megi_webview::render::decode_image_with_svg(svg.as_bytes(), ICON_SIZE)?;
                if image.width == 0 || image.height == 0 {
                    return None;
                }
                let width = usize::try_from(image.width).ok()?;
                let height = usize::try_from(image.height).ok()?;
                let needed = width.checked_mul(height)?.checked_mul(4)?;
                if image.data.len() < needed {
                    return None;
                }
                Some(image)
            })
            .collect();
        Self { masks }
    }

    /// Paints `icon` tinted with `color` over `rect` using source-over.
    ///
    /// `rect` and `scale` are CSS pixels; the destination is device pixels.
    /// Non-finite or negative geometry writes nothing, and `scale` falls back to
    /// `1.0` when it is not finite and positive. Returns the number of pixels
    /// whose alpha mask contributed to the destination.
    #[must_use]
    pub fn paint(
        &self,
        pixmap: &mut Pixmap,
        icon: Icon,
        rect: Rect,
        color: Color,
        scale: f32,
    ) -> usize {
        let Some(Some(mask)) = self.masks.get(icon.index()) else {
            return 0;
        };
        if color.a == 0
            || !rect.x.is_finite()
            || !rect.y.is_finite()
            || !rect.width.is_finite()
            || !rect.height.is_finite()
        {
            return 0;
        }
        let scale = if scale.is_finite() && scale > 0.0 {
            f64::from(scale)
        } else {
            1.0
        };
        let x0 = f64::from(rect.x) * scale;
        let y0 = f64::from(rect.y) * scale;
        let x1 = (f64::from(rect.x) + f64::from(rect.width)) * scale;
        let y1 = (f64::from(rect.y) + f64::from(rect.height)) * scale;
        let left = x0.max(0.0).min(f64::from(pixmap.width));
        let top = y0.max(0.0).min(f64::from(pixmap.height));
        let right = x1.max(0.0).min(f64::from(pixmap.width));
        let bottom = y1.max(0.0).min(f64::from(pixmap.height));
        if right <= left || bottom <= top {
            return 0;
        }
        // Values are clamped to the destination before conversion, so the cast
        // can never truncate a coordinate outside the pixmap.
        let first_x = left.floor() as u32;
        let last_x = right.ceil() as u32;
        let first_y = top.floor() as u32;
        let last_y = bottom.ceil() as u32;
        let span_x = right - left;
        let span_y = bottom - top;
        let mask_width = mask.width;
        let mask_height = mask.height;
        let mut painted = 0usize;
        for dest_y in first_y..last_y {
            // Sample at pixel centers so fractional scales keep the glyph aligned.
            let source_y = ((f64::from(dest_y - first_y) + 0.5) * f64::from(mask_height) / span_y)
                .floor()
                .clamp(0.0, f64::from(mask_height - 1)) as usize;
            for dest_x in first_x..last_x {
                let source_x = ((f64::from(dest_x - first_x) + 0.5) * f64::from(mask_width)
                    / span_x)
                    .floor()
                    .clamp(0.0, f64::from(mask_width - 1)) as usize;
                let Some(offset) = source_y
                    .checked_mul(usize::try_from(mask_width).unwrap_or(0))
                    .and_then(|row| row.checked_add(source_x))
                    .and_then(|index| index.checked_mul(4))
                else {
                    continue;
                };
                let Some(pixel) = mask.data.get(offset..offset.saturating_add(4)) else {
                    continue;
                };
                if pixel.len() != 4 {
                    continue;
                }
                // Straight-alpha mask: keep its coverage, replace its RGB with
                // the requested tint (Porter-Duff source-over).
                let alpha = u16::from(pixel[3]) * u16::from(color.a) / 255;
                if alpha == 0 {
                    continue;
                }
                let alpha = u8::try_from(alpha).unwrap_or(u8::MAX);
                pixmap.blend_pixel(dest_x, dest_y, Color::new(color.r, color.g, color.b, alpha));
                painted += 1;
            }
        }
        painted
    }
}
