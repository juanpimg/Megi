//! Per-pass state for `background-clip: text` collector slots (#2296).
//!
//! A single paint pass may collect at most `MAX_TEXT_MASK_COLLECTORS` glyph
//! masks (#658). That budget must belong to the pass: a process-global counter
//! shared by concurrent `build_display_list` calls loses groups (#2296).

use std::sync::{Arc, Barrier, Mutex};

use megi_core::{Document, Namespace};
use megi_css::{BackgroundClip, BackgroundLayer, Color, ComputedStyle, Display, TextDecoration};
use megi_layout::{BoxType, Dimensions, EdgeSizes, LayoutBox, Rect};
use megi_paint::{
    BackgroundImages, DisplayItem, DisplayList, ImageStore,
    build_display_list_with_images_and_backgrounds,
};

const VIEWPORT: Rect = Rect {
    x: 0.0,
    y: 0.0,
    width: 100.0,
    height: 80.0,
};

fn block_style() -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::Block;
    style
}

fn sized_box(box_type: BoxType, style: ComputedStyle, rect: Rect) -> LayoutBox {
    let mut layout_box = LayoutBox::new(box_type, Some(style));
    layout_box.dimensions = Dimensions {
        content: rect,
        padding: EdgeSizes::ZERO,
        border: EdgeSizes::ZERO,
        margin: EdgeSizes::ZERO,
    };
    layout_box
}

/// A tree whose 128 `background-clip: text` boxes request far more collector
/// slots than the per-pass budget (32, `painter/effects.rs::MAX_TEXT_MASK_COLLECTORS`).
/// Every pass must nevertheless emit exactly the budget's worth of groups.
fn clip_text_stress_tree(node: megi_core::NodeId) -> LayoutBox {
    let mut root_style = block_style();
    root_style.background_color = Color::WHITE;
    let mut root = sized_box(BoxType::BlockNode(node), root_style, VIEWPORT);
    let mut container = sized_box(
        BoxType::BlockNode(node),
        block_style(),
        Rect::new(0.0, 0.0, 100.0, 80.0),
    );
    let mut collector_style = block_style();
    collector_style.background_color = Color::RED;
    collector_style.background_layers = vec![BackgroundLayer {
        clip: BackgroundClip::Text,
        ..BackgroundLayer::default()
    }];
    collector_style.background_geometry_explicit = true;
    let mut text_style = block_style();
    text_style.display = Display::Inline;
    text_style.color = Color::TRANSPARENT;
    text_style.font_size = 32.0;
    text_style.font_family = "DejaVu Sans".to_string();
    text_style.text_decoration = TextDecoration::NONE;
    container.children = (0..128)
        .map(|index| {
            let rect = Rect::new(0.0, index as f32, 10.0, 1.0);
            let mut collector = sized_box(BoxType::BlockNode(node), collector_style.clone(), rect);
            collector.children.push(sized_box(
                BoxType::TextNode("Ab".to_string()),
                text_style.clone(),
                rect,
            ));
            collector
        })
        .collect();
    root.children.push(container);
    root
}

fn count_text_mask_groups(list: &DisplayList) -> usize {
    list.items
        .iter()
        .filter(|item| matches!(item, DisplayItem::PushTextMaskGroup { .. }))
        .count()
}

#[test]
fn test_text_mask_collector_budget_is_per_pass() {
    let mut document = Document::new();
    let html = document.create_element(Namespace::Html, "html", vec![]);
    document.append_child(document.root(), html);
    let node = document.create_element(Namespace::Html, "div", vec![]);
    document.append_child(html, node);
    let tree = clip_text_stress_tree(node);
    let build = || {
        build_display_list_with_images_and_backgrounds(
            &tree,
            VIEWPORT,
            &ImageStore::new(),
            &BackgroundImages::new(),
        )
    };
    // Self-calibrating baseline: one isolated pass must fill the whole budget.
    let expected = count_text_mask_groups(&build());
    assert_eq!(
        expected, 32,
        "baseline should hit the per-pass collector cap"
    );

    // Concurrent passes must each get the same budget; a process-global counter
    // is reset and shared by all of them, so at least one loses its groups.
    const PASSES: usize = 16;
    const ROUNDS: usize = 16;
    let barrier = Arc::new(Barrier::new(PASSES));
    let shortfalls = Arc::new(Mutex::new(Vec::new()));
    std::thread::scope(|scope| {
        for _ in 0..PASSES {
            let barrier = Arc::clone(&barrier);
            let shortfalls = Arc::clone(&shortfalls);
            scope.spawn(move || {
                for _ in 0..ROUNDS {
                    barrier.wait();
                    let groups = count_text_mask_groups(&build());
                    if groups != expected {
                        shortfalls.lock().expect("lock").push(groups);
                    }
                }
            });
        }
    });
    let shortfalls = shortfalls.lock().expect("lock");
    assert!(
        shortfalls.is_empty(),
        "concurrent passes lost mask groups: expected {expected}, got {shortfalls:?} (#2296)"
    );
}
