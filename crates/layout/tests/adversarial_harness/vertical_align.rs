use super::common::*;

#[test]
fn test_adversarial_vertical_align_combinations() {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let p = doc.create_element(Namespace::Html, "p", vec![]);
    let span_base = doc.create_element(Namespace::Html, "span", vec![]);
    let span_sub = doc.create_element(Namespace::Html, "span", vec![]);
    let span_super = doc.create_element(Namespace::Html, "span", vec![]);
    let span_len_pos = doc.create_element(Namespace::Html, "span", vec![]);
    let span_len_neg = doc.create_element(Namespace::Html, "span", vec![]);
    let span_ttop = doc.create_element(Namespace::Html, "span", vec![]);
    let span_tbot = doc.create_element(Namespace::Html, "span", vec![]);

    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, p);

    doc.append_child(p, span_base);
    doc.append_text(span_base, "A ");
    doc.append_child(p, span_sub);
    doc.append_text(span_sub, "B ");
    doc.append_child(p, span_super);
    doc.append_text(span_super, "C ");
    doc.append_child(p, span_len_pos);
    doc.append_text(span_len_pos, "D ");
    doc.append_child(p, span_len_neg);
    doc.append_text(span_len_neg, "E ");
    doc.append_child(p, span_ttop);
    doc.append_text(span_ttop, "F ");
    doc.append_child(p, span_tbot);
    doc.append_text(span_tbot, "G");

    let mut p_style = make_block_style(Length::Px(800.0), Length::Auto);
    p_style.font_size = 20.0;
    p_style.line_height = Some(24.0);

    let mut style_base = make_inline_style();
    style_base.font_size = 20.0;
    style_base.line_height = Some(24.0);
    style_base.vertical_align = VerticalAlign::Baseline;

    let mut style_sub = style_base.clone();
    style_sub.vertical_align = VerticalAlign::Sub;

    let mut style_super = style_base.clone();
    style_super.vertical_align = VerticalAlign::Super;

    let mut style_len_pos = style_base.clone();
    style_len_pos.vertical_align = VerticalAlign::Length(30.0); // shifted up 30px

    let mut style_len_neg = style_base.clone();
    style_len_neg.vertical_align = VerticalAlign::Length(-25.0); // shifted down 25px

    let mut style_ttop = style_base.clone();
    style_ttop.vertical_align = VerticalAlign::TextTop;

    let mut style_tbot = style_base.clone();
    style_tbot.vertical_align = VerticalAlign::TextBottom;

    let mut styles = HashMap::new();
    styles.insert(html, make_block_style(Length::Auto, Length::Auto));
    styles.insert(body, make_block_style(Length::Auto, Length::Auto));
    styles.insert(p, p_style);
    styles.insert(span_base, style_base);
    styles.insert(span_sub, style_sub);
    styles.insert(span_super, style_super);
    styles.insert(span_len_pos, style_len_pos);
    styles.insert(span_len_neg, style_len_neg);
    styles.insert(span_ttop, style_ttop);
    styles.insert(span_tbot, style_tbot);

    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let layout_tree = layout_document(&doc, &styles, viewport).expect("layout tree");

    let p_box = &layout_tree.children[0].children[0];
    assert_eq!(p_box.children.len(), 1);
    let line_box = &p_box.children[0];

    // Strut: font 20, line_height 24 -> ascent 16, descent 4, leading 4. strut_ascent = 18, strut_descent = 6.
    // Length(+30): a_prime = 18 + 30 = 48. max_ascent = 48.
    // Length(-25): d_prime = 6 - (-25) = 31. max_descent = 31.
    // Total line height = 48 + 31 = 79.0px!
    println!("Line box height: {}", line_box.dimensions.content.height);
    assert_eq!(line_box.dimensions.content.height, 79.0);
    assert_eq!(p_box.dimensions.content.height, 79.0);

    // Verify all child fragments lie within [line_y, line_y + line_height]
    let line_y = line_box.dimensions.content.y;
    let line_h = line_box.dimensions.content.height;
    for child in &line_box.children {
        // Child can be InlineNode containing TextNode
        let text_child = if !child.children.is_empty() {
            &child.children[0]
        } else {
            child
        };
        let top = text_child.dimensions.content.y;
        let bottom = top + text_child.dimensions.content.height;
        println!(
            "Fragment top={}, bottom={}, bounds=[{}, {}]",
            top,
            bottom,
            line_y,
            line_y + line_h
        );
        assert!(
            top >= line_y - 0.001,
            "Top {} should be >= line_y {}",
            top,
            line_y
        );
        assert!(
            bottom <= line_y + line_h + 0.001,
            "Bottom {} should be <= line_y + h {}",
            bottom,
            line_y + line_h
        );
    }
}

#[test]
fn test_adversarial_vertical_align_middle_bounds() {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let p = doc.create_element(Namespace::Html, "p", vec![]);
    let span_mid = doc.create_element(Namespace::Html, "span", vec![]);

    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, p);
    doc.append_child(p, span_mid);
    doc.append_text(span_mid, "Middle");

    // Tall line box so middle alignment is distinguishable from baseline (#1382).
    let mut p_style = make_block_style(Length::Px(400.0), Length::Auto);
    p_style.font_size = 16.0;
    p_style.line_height = Some(40.0);

    let mut mid_style = make_inline_style();
    mid_style.font_size = 24.0;
    mid_style.line_height = Some(30.0);
    mid_style.vertical_align = VerticalAlign::Middle;

    let mut styles = HashMap::new();
    styles.insert(html, make_block_style(Length::Auto, Length::Auto));
    styles.insert(body, make_block_style(Length::Auto, Length::Auto));
    styles.insert(p, p_style);
    styles.insert(span_mid, mid_style);

    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let layout_tree = layout_document(&doc, &styles, viewport).expect("layout tree");

    let p_box = &layout_tree.children[0].children[0];
    let line_box = &p_box.children[0];
    let span_box = &line_box.children[0];
    let text_box = &span_box.children[0];

    let line_y = line_box.dimensions.content.y;
    let line_h = line_box.dimensions.content.height;
    let top = text_box.dimensions.content.y;
    let fragment_h = text_box.dimensions.content.height;

    // CSS 2.1 §10.8.1: middle aligns the box midpoint with parent baseline + 0.5*x-height.
    // This is not the same as geometric centering in the line box, but it must differ from
    // baseline alignment; the old bounds-only check accepted a baseline-positioned fragment.
    let parent_x_height_midpoint = 0.25 * 16.0;
    let expected_middle =
        line_y + line_box.baseline - (parent_x_height_midpoint + fragment_h / 2.0);

    let fragment_font_size = 24.0_f32;
    let fragment_line_height = 30.0_f32;
    let fragment_ascent = 0.8 * fragment_font_size;
    let fragment_descent = 0.2 * fragment_font_size;
    let fragment_leading = fragment_line_height - (fragment_ascent + fragment_descent);
    let fragment_ascent_extent = fragment_ascent + fragment_leading / 2.0;
    let baseline_y = line_y + line_box.baseline - fragment_ascent_extent;

    assert!(
        top >= line_y - 0.001 && top + fragment_h <= line_y + line_h + 0.001,
        "Middle fragment must remain within line box bounds (top={top}, bottom={}, line=[{line_y}, {}])",
        top + fragment_h,
        line_y + line_h
    );
    assert!(
        (top - expected_middle).abs() < 0.01,
        "Middle fragment top {top} must follow vertical-align: middle (expected {expected_middle}, baseline would be {baseline_y}, line_y={line_y}, line_h={line_h}, line_baseline={}, fragment_h={fragment_h})",
        line_box.baseline
    );
    assert!(
        (top - baseline_y).abs() > 0.01,
        "Middle-aligned fragment must not remain on the baseline (top {top}, baseline_y {baseline_y})"
    );
}

#[test]
fn test_adversarial_vertical_align_middle_heterogeneous_font_sizes() {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let p = doc.create_element(Namespace::Html, "p", vec![]);
    let span_normal = doc.create_element(Namespace::Html, "span", vec![]);
    let span_large = doc.create_element(Namespace::Html, "span", vec![]);
    let span_small = doc.create_element(Namespace::Html, "span", vec![]);

    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, p);
    doc.append_child(p, span_normal);
    doc.append_text(span_normal, "Normal ");
    doc.append_child(p, span_large);
    doc.append_text(span_large, "Large ");
    doc.append_child(p, span_small);
    doc.append_text(span_small, "Small");

    let mut p_style = make_block_style(Length::Px(800.0), Length::Auto);
    p_style.font_size = 16.0;
    p_style.line_height = Some(50.0);

    let mut normal_style = make_inline_style();
    normal_style.font_size = 16.0;
    normal_style.line_height = Some(20.0);
    normal_style.vertical_align = VerticalAlign::Baseline;

    let mut large_style = make_inline_style();
    large_style.font_size = 32.0;
    large_style.line_height = Some(40.0);
    large_style.vertical_align = VerticalAlign::Middle;

    let mut small_style = make_inline_style();
    small_style.font_size = 8.0;
    small_style.line_height = Some(10.0);
    small_style.vertical_align = VerticalAlign::Middle;

    let mut styles = HashMap::new();
    styles.insert(html, make_block_style(Length::Auto, Length::Auto));
    styles.insert(body, make_block_style(Length::Auto, Length::Auto));
    styles.insert(p, p_style);
    styles.insert(span_normal, normal_style);
    styles.insert(span_large, large_style);
    styles.insert(span_small, small_style);

    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let layout_tree = layout_document(&doc, &styles, viewport).expect("layout tree");

    let p_box = &layout_tree.children[0].children[0];
    let line_box = &p_box.children[0];
    let line_y = line_box.dimensions.content.y;
    let line_h = line_box.dimensions.content.height;

    println!("LineBox: y={}, h={}", line_y, line_h);

    let parent_x_height_midpoint = 0.25 * 16.0;
    let fragment_metrics = [
        (16.0_f32, 20.0_f32), // normal (baseline)
        (32.0_f32, 40.0_f32), // large (middle)
        (8.0_f32, 10.0_f32),  // small (middle)
    ];

    for (index, span_child) in line_box.children.iter().enumerate() {
        for text_child in &span_child.children {
            let top = text_child.dimensions.content.y;
            let fragment_h = text_child.dimensions.content.height;
            if index == 0 {
                // "Normal" uses baseline alignment; only bound it within the line box.
                let bottom = top + fragment_h;
                assert!(
                    top >= line_y - 0.001,
                    "Baseline fragment top {top} must be within line box (line_y = {line_y})"
                );
                assert!(
                    bottom <= line_y + line_h + 0.001,
                    "Baseline fragment bottom {bottom} must be within line box (line_bottom = {})",
                    line_y + line_h
                );
            } else {
                // Middle aligns midpoint with parent baseline + 0.5*x-height (CSS 2.1 §10.8.1),
                // which differs from baseline in this mixed-size line (#1382).
                let (font_size, fragment_line_height) = fragment_metrics[index];
                let ascent = 0.8 * font_size;
                let descent = 0.2 * font_size;
                let leading = fragment_line_height - (ascent + descent);
                let ascent_extent = ascent + leading / 2.0;
                let expected_middle =
                    line_y + line_box.baseline - (parent_x_height_midpoint + fragment_h / 2.0);
                let baseline_y = line_y + line_box.baseline - ascent_extent;
                assert!(
                    top >= line_y - 0.001 && top + fragment_h <= line_y + line_h + 0.001,
                    "Middle fragment must remain within line box bounds (top={top}, bottom={}, line=[{line_y}, {}], span_index={index})",
                    top + fragment_h,
                    line_y + line_h
                );
                assert!(
                    (top - expected_middle).abs() < 0.01,
                    "Middle fragment top {top} must follow vertical-align: middle (expected {expected_middle}, baseline would be {baseline_y}, line_y={line_y}, line_h={line_h}, line_baseline={}, fragment_h={fragment_h}, span_index={index})",
                    line_box.baseline
                );
                assert!(
                    (top - baseline_y).abs() > 0.01,
                    "Middle-aligned fragment must not remain on the baseline (top {top}, baseline_y {baseline_y}, span_index={index})"
                );
            }
        }
    }
}
