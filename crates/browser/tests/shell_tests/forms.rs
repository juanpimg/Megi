use super::common::*;

#[test]
fn forms_fixture_renders_visible_control_borders() {
    let output = megi_browser::render_target(&forms_fixture_url(), 640, 480).expect("forms render");
    assert!(matches!(output.dimensions(), (640, 480)));
    // The UA stylesheet draws 1px #767676 borders and #efefef buttons. 1px edges are
    // antialiased, so count the grayscale bands around those colors instead of exact RGB.
    let mut border_pixels = 0usize;
    let mut button_pixels = 0usize;
    for pixel in &output.pixmap.pixels {
        let (r, g, b) = (pixel.r, pixel.g, pixel.b);
        let gray = r == g && g == b;
        if gray && (0x50..=0xA0).contains(&r) {
            border_pixels += 1;
        }
        if gray && (0xD0..=0xF5).contains(&r) {
            button_pixels += 1;
        }
    }
    assert!(
        border_pixels > 300,
        "expected input/fieldset borders, got {border_pixels}"
    );
    assert!(
        button_pixels > 500,
        "expected button backgrounds, got {button_pixels}"
    );

    let request = RenderRequest {
        generation: 1,
        input: forms_fixture_url(),
        css_width: 640,
        css_height: 480,
        raster_scale: 1.0,
        base_override: None,
        scripting: true,
        find_query: None,
        host_events: Vec::new(),
        host_dom_events: Vec::new(),
        form_post: None,
    };
    let frame = megi_browser::worker::render_document(&request).expect("worker forms render");
    assert!(!frame.forms.is_empty(), "frame must expose form controls");
    assert!(
        frame
            .forms
            .iter()
            .any(|control| control.kind == megi_browser::FormControlKind::Textarea)
    );
    assert!(frame.forms.iter().any(|control| control.kind.is_button()));
}

#[test]
fn patched_dom_snapshot_re_renders_edited_values() {
    let request = RenderRequest {
        generation: 1,
        input: forms_fixture_url(),
        css_width: 320,
        css_height: 240,
        raster_scale: 1.0,
        base_override: None,
        scripting: true,
        find_query: None,
        host_events: Vec::new(),
        host_dom_events: Vec::new(),
        form_post: None,
    };
    let frame = megi_browser::worker::render_document(&request).expect("initial render");
    let text = frame
        .forms
        .iter()
        .find(|control| control.kind == megi_browser::FormControlKind::TextInput)
        .cloned()
        .expect("text input");
    let mut document = (*frame.document).clone();
    megi_browser::forms::assign_form_ids(&mut document);
    assert!(megi_browser::forms::patch_control(
        &mut document,
        text.form_id,
        Some("Edited"),
        None,
        true,
        false
    ));
    let snapshot = megi_browser::forms::snapshot_data_url(&document);
    let patched_request = RenderRequest {
        generation: 2,
        input: snapshot,
        css_width: 320,
        css_height: 240,
        raster_scale: 1.0,
        base_override: Some(forms_fixture_url()),
        scripting: false,
        find_query: None,
        host_events: Vec::new(),
        host_dom_events: Vec::new(),
        form_post: None,
    };
    let patched = megi_browser::worker::render_document(&patched_request).expect("patched render");
    let edited = patched
        .forms
        .iter()
        .find(|control| control.form_id == text.form_id)
        .expect("same stable form id after re-render");
    assert_eq!(edited.value, "Edited");
    assert!(
        patched
            .document
            .get_elements_by_tag_name("input")
            .iter()
            .any(|id| patched
                .document
                .node(*id)
                .as_element()
                .is_some_and(|element| element.get_attribute(megi_browser::FOCUS_ATTR).is_some()))
    );
    // The focus ring is a UA-style border (`[data-megi-focused]`, asserted by the css tests);
    // the widget-less rasterizer paints no form text, so the visible state of the frame is
    // the control box plus its focus border. The DOM-level assertions above are the contract.
    let _ = frame;
}

#[test]
fn form_editor_state_and_cli_help_are_documented() {
    let mut focus = megi_browser::FocusState::new();
    assert!(focus.next(3));
    assert_eq!(focus.focused(), Some(0));
    let mut edit = megi_browser::TextEdit::new("ab");
    assert!(edit.insert("c"));
    assert_eq!(edit.value(), "abc");
    assert!(edit.backspace());
    assert_eq!(edit.value(), "ab");
    assert!(megi_browser::cli::USAGE.contains("Tab"));
    assert!(megi_browser::cli::USAGE.contains("form control")); // documented control
}
