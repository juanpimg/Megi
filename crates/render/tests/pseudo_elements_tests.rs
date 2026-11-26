//! End-to-end render pipeline tests for CSS `::before` and `::after` pseudo-elements (#1540).

use megi_render::{RenderCache, RenderOptions, Viewport, render_url, render_url_cached};

fn data_url(html: &str) -> String {
    let mut encoded = String::with_capacity(html.len());
    for byte in html.bytes() {
        match byte {
            b'#' => encoded.push_str("%23"),
            b'%' => encoded.push_str("%25"),
            b'?' => encoded.push_str("%3F"),
            b'\n' => encoded.push_str("%0A"),
            b'\r' => encoded.push_str("%0D"),
            b' ' => encoded.push_str("%20"),
            _ if byte.is_ascii() => encoded.push(byte as char),
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    format!("data:text/html;charset=utf-8,{encoded}")
}

fn count_color(pixmap: &megi_paint::Pixmap, color: megi_css::Color) -> usize {
    pixmap
        .pixels
        .iter()
        .filter(|pixel| **pixel == color)
        .count()
}

#[test]
fn pseudo_element_backgrounds_and_html_before_paint_in_png() {
    let html = r#"<!doctype html>
    <html>
    <head><style>
        body { margin: 0; background: rgb(255, 255, 255); }
        html::before {
            content: "";
            display: block;
            width: 40px;
            height: 20px;
            background: rgb(255, 0, 0);
        }
        #box::after {
            content: "";
            display: block;
            width: 30px;
            height: 30px;
            background: rgb(0, 128, 0);
        }
    </style></head>
    <body><div id="box"></div></body>
    </html>"#;

    let options = RenderOptions {
        viewport: Viewport::new(200, 150).expect("viewport"),
        ..RenderOptions::default()
    };
    let output = render_url(&data_url(html), &options).expect("render");

    let red = count_color(&output.pixmap, megi_css::Color::RED);
    let green = count_color(&output.pixmap, megi_css::Color::GREEN);
    assert!(
        red >= 40 * 20,
        "html::before must paint its 40x20 red box (not skipped by canvas propagation), got {red} px"
    );
    assert!(
        green >= 30 * 30,
        "#box::after must paint its 30x30 green box, got {green} px"
    );

    // Clicking inside #box::after maps hit region back to the originating #box element.
    let box_id = output.document.get_element_by_id("box").expect("#box");
    let hit = megi_paint::hit_test(&output.hit_regions, 10.0, 35.0);
    assert_eq!(hit, Some(box_id));
}

#[test]
fn render_cache_distinguishes_changed_pseudo_element_styles() {
    let html_a = r#"<!doctype html><html><head><style>
        body { margin: 0; }
        div::before { content: "A"; display: block; height: 20px; }
    </style></head><body><div></div></body></html>"#;
    let html_b = r#"<!doctype html><html><head><style>
        body { margin: 0; }
        div::before { content: "B"; display: block; height: 60px; }
    </style></head><body><div></div></body></html>"#;

    let options = RenderOptions {
        viewport: Viewport::new(200, 150).expect("viewport"),
        ..RenderOptions::default()
    };
    let mut cache = RenderCache::new();
    let out_a = render_url_cached(&data_url(html_a), &options, &mut cache).expect("render a");
    let out_b = render_url_cached(&data_url(html_b), &options, &mut cache).expect("render b");

    let div_a = out_a.document.get_elements_by_tag_name("div")[0];
    let div_b = out_b.document.get_elements_by_tag_name("div")[0];

    fn box_height(layout: &megi_layout::LayoutBox, id: megi_core::NodeId) -> f32 {
        if layout.node_id() == Some(id) && !layout.is_pseudo {
            return layout.dimensions.content.height;
        }
        for c in &layout.children {
            let h = box_height(c, id);
            if h > 0.0 {
                return h;
            }
        }
        0.0
    }

    assert!((box_height(&out_a.layout, div_a) - 20.0).abs() < 0.5);
    assert!((box_height(&out_b.layout, div_b) - 60.0).abs() < 0.5);
}

#[cfg(feature = "js")]
#[test]
fn js_get_bounding_client_rect_uses_originating_element_not_pseudo_box() {
    let html = r#"<!doctype html>
    <html><head><style>
        body { margin: 0; }
        #target { width: 100px; height: 50px; }
        #target::before { content: ""; display: block; width: 10px; height: 10px; }
    </style></head>
    <body>
        <div id="target"></div>
        <script>
            const r = document.getElementById('target').getBoundingClientRect();
            console.log(r.width + 'x' + r.height);
        </script>
    </body></html>"#;

    let options = RenderOptions {
        viewport: Viewport::new(200, 150).expect("viewport"),
        ..RenderOptions::default()
    };
    let output = render_url(&data_url(html), &options).expect("render");
    let log = output
        .stats
        .console
        .iter()
        .find(|entry| entry.message.contains("100x50"));
    assert!(
        log.is_some(),
        "getBoundingClientRect must return originating 100x50 box, got console: {:?}",
        output.stats.console
    );
}

#[test]
fn absolute_after_does_not_widen_its_inline_anchor() {
    // CSS 2.2 §9.6: an absolutely positioned box is out of flow and must not
    // advance the inline cursor of its parent. Wikipedia's tab underlines are
    // `a::after { position:absolute; left:0; width:100% }`; painting them in flow
    // after the label pushes the underline into the neighbouring tab.
    let html = r#"<!doctype html><html><head><style>
        body { margin: 0; font: 14px sans-serif; }
        ul { display: flex; }
        li { white-space: nowrap; margin: 0 8px; }
        a { display: inline-flex; position: relative; }
        a::after { content: ''; position: absolute; left: 0; bottom: 0;
                   width: 100%; height: 2px; background: rgb(0,0,255); }
    </style></head><body><ul>
        <li><a href="one"><span>Primera pestaña</span></a></li>
        <li><a href="two"><span>Segunda</span></a></li>
    </ul></body></html>"#;
    let options = RenderOptions {
        viewport: Viewport::new(320, 120).expect("viewport"),
        ..RenderOptions::default()
    };
    let output = render_url(&data_url(html), &options).expect("render tabs");
    let anchors = output.document.get_elements_by_tag_name("a");
    assert_eq!(anchors.len(), 2);

    for (i, id) in anchors.iter().enumerate() {
        let anchor_x = output
            .hit_regions
            .iter()
            .filter(|region| region.node_id == Some(*id) && region.rect.height > 5.0)
            .map(|region| region.rect.x)
            .next()
            .expect("anchor box");
        let text_width: f32 = output
            .hit_regions
            .iter()
            .filter(|region| {
                region.kind == megi_display::HitKind::Text && (region.rect.x - anchor_x).abs() < 1.0
            })
            .map(|region| region.rect.width)
            .sum();
        let own_widths: Vec<f32> = output
            .hit_regions
            .iter()
            .filter(|region| region.node_id == Some(*id) && region.rect.height > 5.0)
            .map(|region| region.rect.width)
            .collect();
        // CSS 2.2 §9.4.2: the inline box spans its own content, not the following
        // sibling's. Before the fix it absorbed the out-of-flow `::after` width.
        assert!(
            own_widths.iter().all(|width| *width <= text_width + 1.0),
            "anchor {i} inline box {own_widths:?} wider than its text {text_width}"
        );
        let painted_right: Vec<f32> = output
            .hit_regions
            .iter()
            .filter(|region| region.node_id == Some(*id))
            .map(|region| region.rect.x + region.rect.width)
            .collect();
        let next_x = anchors
            .get(i + 1)
            .and_then(|next| {
                output
                    .hit_regions
                    .iter()
                    .find(|region| region.node_id == Some(*next) && region.rect.height > 5.0)
                    .map(|region| region.rect.x)
            })
            .unwrap_or(320.0);
        assert!(
            painted_right.iter().all(|right| *right < next_x),
            "anchor {i} painted rects end at {painted_right:?}, overlapping next tab at {next_x}"
        );
    }
}
