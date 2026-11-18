//! End-to-end tests for the Phase 4 `HTML → styles → layout → display list →
//! PNG` pipeline (WP-4.1).
//!
//! Rendering path under test (same one as `examples/html_to_png.rs`):
//!
//! ```text
//! megi_html::parse_document
//!   ├─ megi_paint::collect_style_text
//!   ├─ megi_css::resolve_document_styles
//!   ├─ megi_layout::layout_document
//!   ├─ megi_paint::build_display_list
//!   ├─ megi_paint::rasterize_display_list
//!   └─ megi_paint::Pixmap::to_png
//! ```

#[path = "../common/mod.rs"]
mod common;

mod pipeline;
mod support;
mod text_and_fonts;
