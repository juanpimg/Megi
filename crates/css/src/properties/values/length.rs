//! CSS `<length>` value types and their parsers.

/// CSS Length dimension.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Length {
    Px(f32),
    Percent(f32),
    Auto,
}

fn scaled_px(raw: &str, mul: f32, div: f32) -> Option<Length> {
    let value: f32 = raw.trim().parse().ok()?;
    if !value.is_finite() || value.abs() > crate::calc::MAX_LENGTH_MAGNITUDE {
        return None;
    }
    let px = value * mul / div;
    (px.is_finite() && px.abs() <= crate::calc::MAX_LENGTH_MAGNITUDE).then_some(Length::Px(px))
}

impl Length {
    pub const ZERO: Self = Self::Px(0.0);

    pub fn parse(s: &str, font_size: f32, root_font_size: f32) -> Option<Self> {
        let s = s.trim().to_ascii_lowercase();
        // CSS Values 4 §11: calc()/min()/max()/clamp() are <length> math functions.
        if ["calc(", "min(", "max(", "clamp("]
            .iter()
            .any(|prefix| s.starts_with(prefix))
        {
            return crate::calc::evaluate_length(s.as_str(), font_size, root_font_size);
        }
        if s == "auto" {
            return Some(Self::Auto);
        }
        if s == "0" {
            return Some(Self::Px(0.0));
        }
        // Non-finite numbers (`1e400` parses as `inf`) are invalid CSS values: accept them
        // would poison percentage resolution (`0 * inf / 100 = NaN`) and the layout math.
        if let Some(val) = s.strip_suffix("px") {
            return scaled_px(val, 1.0, 1.0);
        }
        if let Some(val) = s.strip_suffix("rem") {
            return scaled_px(val, root_font_size, 1.0);
        }
        if let Some(val) = s.strip_suffix("em") {
            return scaled_px(val, font_size, 1.0);
        }
        if let Some(val) = s.strip_suffix("pt") {
            return scaled_px(val, 96.0, 72.0);
        }
        // CSS Values 4 §5.1.2: viewport-percentage lengths are <length> units and must be
        // accepted standalone, not only inside calc(). #1927: width:50vw used to fall back
        // to `auto` (full width) because only calc() resolved these units. calc.rs already
        // resolves vw/vh against the same thread-local viewport set by the render pipeline.
        if let Some(val) = s.strip_suffix("vw") {
            let (width, _) = crate::calc::viewport();
            return scaled_px(val, width, 100.0);
        }
        if let Some(val) = s.strip_suffix("vh") {
            let (_, height) = crate::calc::viewport();
            return scaled_px(val, height, 100.0);
        }
        if let Some(val) = s.strip_suffix('%') {
            let pct: f32 = val.trim().parse().ok()?;
            if !pct.is_finite() || pct.abs() > crate::calc::MAX_LENGTH_MAGNITUDE {
                return None;
            }
            return Some(Self::Percent(pct));
        }

        // Raw number without unit: only 0 is valid per CSS Values 3/4.
        if let Ok(px) = s.parse::<f32>() {
            if px == 0.0 {
                return Some(Self::Px(0.0));
            }
            return None;
        }

        None
    }
}

/// Mixed `calc()` side values (`percentage + absolute length`) for properties that
/// accept a `<length-percentage>`.
///
/// `Length` itself intentionally stays `Px`/`Percent`/`Auto` (paint/render match it
/// exhaustively); when a declaration computes to a *mixed* `calc()` value the plain
/// `Length` stores the absolute component as a fallback and the full value is kept
/// here. Layout resolves `percent * basis / 100 + px` (percentage part is `0` when the
/// axis basis is indefinite, documented approximation).
///
/// Limitations: `font-size` only accepts absolute lengths (mixed `calc()` there stays
/// invalid); grid track sizes (`grid-template-*`/`minmax()`) still take pure lengths;
/// `background-position`/`background-size`/`border-radius` are computed here but the
/// paint crate keeps consuming the plain `Length` fields, so mixed calc there is parsed
/// and stored but not yet painted (documented, paint out of this task's scope).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CalcLengths {
    pub width: Option<crate::calc::CalcValue>,
    pub height: Option<crate::calc::CalcValue>,
    pub min_width: Option<crate::calc::CalcValue>,
    pub max_width: Option<crate::calc::CalcValue>,
    pub min_height: Option<crate::calc::CalcValue>,
    pub max_height: Option<crate::calc::CalcValue>,
    pub margin_top: Option<crate::calc::CalcValue>,
    pub margin_right: Option<crate::calc::CalcValue>,
    pub margin_bottom: Option<crate::calc::CalcValue>,
    pub margin_left: Option<crate::calc::CalcValue>,
    pub padding_top: Option<crate::calc::CalcValue>,
    pub padding_right: Option<crate::calc::CalcValue>,
    pub padding_bottom: Option<crate::calc::CalcValue>,
    pub padding_left: Option<crate::calc::CalcValue>,
    pub top: Option<crate::calc::CalcValue>,
    pub right: Option<crate::calc::CalcValue>,
    pub bottom: Option<crate::calc::CalcValue>,
    pub left: Option<crate::calc::CalcValue>,
    pub row_gap: Option<crate::calc::CalcValue>,
    pub column_gap: Option<crate::calc::CalcValue>,
    /// First background layer `background-position` x/y components.
    pub background_position_x: Option<crate::calc::CalcValue>,
    pub background_position_y: Option<crate::calc::CalcValue>,
    /// First background layer `background-size` width/height components.
    pub background_size_width: Option<crate::calc::CalcValue>,
    pub background_size_height: Option<crate::calc::CalcValue>,
    /// `border-radius` corners as `[tl_x, tl_y, tr_x, tr_y, br_x, br_y, bl_x, bl_y]`.
    pub border_radius: Option<[crate::calc::CalcValue; 8]>,
}
