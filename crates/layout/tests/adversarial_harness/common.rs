pub(crate) use std::collections::HashMap;

pub(crate) use megi_core::{Document, Namespace};

pub(crate) use megi_css::{ComputedStyle, Display, Length, TextAlign, VerticalAlign, WhiteSpace};

pub(crate) use megi_layout::{BoxType, Rect, layout_document};

pub(crate) fn make_block_style(width: Length, height: Length) -> ComputedStyle {
    let mut s = ComputedStyle::initial();
    s.display = Display::Block;
    s.width = width;
    s.height = height;
    s
}

pub(crate) fn make_inline_style() -> ComputedStyle {
    let mut s = ComputedStyle::initial();
    s.display = Display::Inline;
    s
}
