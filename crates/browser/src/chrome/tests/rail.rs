//! Unit tests for the pure vertical tab rail (Phase 1 visual slice).

use super::*;
use megi_webview::layout::Rect;

fn rail_view<'a>(
    labels: &'a [String],
    active: usize,
    expanded: bool,
    width_css: f32,
) -> RailView<'a> {
    RailView {
        labels,
        active,
        hover: None,
        expanded,
        width_css,
    }
}

fn three_labels() -> Vec<String> {
    vec!["Alpha".to_string(), "Beta".to_string(), "Gamma".to_string()]
}

fn center(rect: Rect) -> (f32, f32) {
    (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
}

#[test]
fn rail_layout_collapses_and_expands() {
    let labels = three_labels();
    let collapsed = layout_rail(
        640.0,
        400.0,
        1.0,
        &rail_view(&labels, 0, false, RAIL_COLLAPSED_WIDTH),
    );
    assert_eq!(collapsed.rect.x, 0.0);
    assert_eq!(collapsed.rect.y, CHROME_HEIGHT, "rail starts below the band");
    assert_eq!(collapsed.rect.width, RAIL_COLLAPSED_WIDTH);
    assert_eq!(collapsed.rect.height, 400.0 - CHROME_HEIGHT);
    assert_eq!(collapsed.tabs.len(), 3);
    for tab in &collapsed.tabs {
        assert!(tab.title.is_empty(), "collapsed rail hides titles");
        assert_eq!(tab.close, Rect::ZERO, "collapsed rail hides close targets");
        assert!(
            tab.rect.height >= 22.0,
            "a collapsed row is at least 22 physical px: {}",
            tab.rect.height
        );
    }

    let expanded = layout_rail(
        640.0,
        400.0,
        1.0,
        &rail_view(&labels, 1, true, RAIL_EXPANDED_WIDTH),
    );
    assert_eq!(expanded.rect.width, RAIL_EXPANDED_WIDTH);
    for tab in &expanded.tabs {
        assert_eq!(tab.title, labels[tab.index], "expanded rail shows full title");
        assert!(
            tab.rect.height >= 22.0,
            "an expanded row is at least 22 physical px"
        );
        let (x, y) = center(tab.close);
        assert!(
            contains_rect(expanded.rect, x, y),
            "close target inside the rail: {tab:?}"
        );
    }
    for (x, y) in [center(expanded.toggle), center(expanded.new_tab)] {
        assert!(
            contains_rect(expanded.rect, x, y),
            "toggle/new-tab inside the rail"
        );
    }
    let (_, toggle_y) = center(expanded.toggle);
    let (_, first_tab_y) = center(expanded.tabs[0].rect);
    assert!(toggle_y < first_tab_y, "toggle is the first row");

    // A fractional scale still yields >= 22 physical px per row.
    let half_scale = layout_rail(
        640.0,
        400.0,
        0.5,
        &rail_view(&labels, 0, false, RAIL_COLLAPSED_WIDTH),
    );
    assert!(
        half_scale.tabs[0].rect.height * 0.5 >= 22.0,
        "row height honours the physical-pixel floor"
    );

    // A viewport shorter than the band must not produce negative geometry.
    let tiny = layout_rail(8.0, 30.0, 1.0, &rail_view(&labels, 0, true, 220.0));
    assert!(tiny.rect.width >= 0.0 && tiny.rect.height >= 0.0);
    assert!(tiny.rect.width <= 8.0, "rail is clamped to the viewport");
    for rect in std::iter::once(tiny.toggle)
        .chain(std::iter::once(tiny.new_tab))
        .chain(tiny.tabs.iter().map(|tab| tab.rect))
    {
        assert!(rect.width >= 0.0 && rect.height >= 0.0, "no negative rect");
    }
}

#[test]
fn rail_hit_distinguishes_tabs_close_new_and_toggle() {
    let labels = three_labels();
    let view = rail_view(&labels, 0, true, RAIL_EXPANDED_WIDTH);
    let layout = layout_rail(640.0, 400.0, 1.0, &view);

    let (x, y) = center(layout.tabs[1].rect);
    assert_eq!(rail_hit(&layout, x, y), Some(RailHit::Activate(1)));

    let (x, y) = center(layout.tabs[0].close);
    assert_eq!(
        rail_hit(&layout, x, y),
        Some(RailHit::Close(0)),
        "close wins over activate on an overlapping point"
    );

    let (x, y) = center(layout.new_tab);
    assert_eq!(rail_hit(&layout, x, y), Some(RailHit::NewTab));
    let (x, y) = center(layout.toggle);
    assert_eq!(rail_hit(&layout, x, y), Some(RailHit::Toggle));

    assert_eq!(rail_hit(&layout, 300.0, 100.0), None, "outside the rail");
    assert_eq!(
        rail_hit(&layout, 5.0, 20.0),
        None,
        "the top band belongs to the horizontal chrome"
    );
}

#[test]
fn rail_paint_is_deterministic_and_does_not_touch_page_rows() {
    use megi_webview::paint::{Color, Pixmap};

    let labels = three_labels();
    let view = rail_view(&labels, 0, true, RAIL_EXPANDED_WIDTH);
    let layout = layout_rail(640.0, 400.0, 1.0, &view);
    let mut first = Pixmap::new(640, 400);
    let mut second = Pixmap::new(640, 400);
    paint_rail(&mut first, 1.0, &layout, &view);
    paint_rail(&mut second, 1.0, &layout, &view);
    assert_eq!(first.pixels, second.pixels, "rail painting is deterministic");

    let mut base = Pixmap::new(640, 400);
    for (index, pixel) in base.pixels.iter_mut().enumerate() {
        *pixel = Color::new((index & 0xFF) as u8, ((index >> 8) & 0xFF) as u8, 0x33, 0xFF);
    }
    let mut painted = base.clone();
    paint_rail(&mut painted, 1.0, &layout, &view);
    assert_ne!(base.pixels, painted.pixels, "the rail must be visible");
    for (index, (before, after)) in base.pixels.iter().zip(&painted.pixels).enumerate() {
        let x = (index % 640) as f32;
        let y = (index / 640) as f32;
        if y < CHROME_HEIGHT || x >= layout.rect.width {
            assert_eq!(
                before, after,
                "rail touched a page pixel outside its columns at ({x},{y})"
            );
        }
    }
}

#[test]
fn rail_tabs_expose_same_labels_as_tab_strip() {
    let labels = three_labels();
    let strip = layout_tabs(&labels, 640.0, 1);
    let view = rail_view(&labels, 1, true, RAIL_EXPANDED_WIDTH);
    let rail = layout_rail(640.0, 400.0, 1.0, &view);
    assert_eq!(rail.tabs.len(), strip.tabs.len());
    for (rail_tab, strip_tab) in rail.tabs.iter().zip(&strip.tabs) {
        assert_eq!(rail_tab.index, strip_tab.index);
        assert_eq!(
            rail_tab.title, strip_tab.title,
            "rail and strip expose the same accessible tab name"
        );
    }
}
