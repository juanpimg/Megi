use super::*;
use megi_webview::core::Namespace;

fn control(form_id: u32, kind: FormControlKind, value: &str) -> FormControl {
    FormControl {
        form_id,
        name: String::new(),
        node_id: NodeId::from_index(0),
        kind,
        rect: CssRect::new(0.0, 0.0, 10.0, 10.0),
        value: value.to_owned(),
        checked: false,
        disabled: false,
        options: Vec::new(),
        multiple: false,
    }
}

fn named(name: &str, kind: FormControlKind, value: &str) -> FormControl {
    let mut control = control(0, kind, value);
    control.name = name.to_owned();
    control
}

#[test]
fn form_submission_reads_action_method_and_serializes_controls() {
    // #1538: a form's submission collects its successful controls and its action/method.
    let mut document = Document::new();
    let html = document.create_element(Namespace::Html, "html", vec![]);
    document.append_child(document.root(), html);
    let body = document.create_element(Namespace::Html, "body", vec![]);
    document.append_child(html, body);
    let form = document.create_element(
        Namespace::Html,
        "form",
        vec![
            Attribute::new("action", "/login"),
            Attribute::new("method", "POST"),
        ],
    );
    document.append_child(body, form);
    let user = document.create_element(
        Namespace::Html,
        "input",
        vec![
            Attribute::new("name", "user"),
            Attribute::new("value", "alice"),
        ],
    );
    document.append_child(form, user);
    let mut user_control = control(1, FormControlKind::TextInput, "alice");
    user_control.node_id = user;
    user_control.name = "user".to_owned();
    let base = megi_webview::net::Url::parse("https://example.com/page").expect("base");
    let submission = form_submission(&document, &base, form, &[user_control]).expect("submission");
    assert_eq!(submission.action, "https://example.com/login");
    assert_eq!(submission.method, "post");
    assert_eq!(submission.body, "user=alice");
}

#[test]
fn form_get_target_appends_query() {
    // #1538: GET submissions append the urlencoded body to the action URL.
    let submission = FormSubmission {
        action: "https://example.com/search".to_owned(),
        method: "get".to_owned(),
        body: "q=megi+browser".to_owned(),
    };
    assert_eq!(
        form_get_target(&submission).as_deref(),
        Some("https://example.com/search?q=megi+browser")
    );
    let post = FormSubmission {
        method: "post".to_owned(),
        ..submission
    };
    assert_eq!(
        form_get_target(&post).as_deref(),
        Some("https://example.com/search")
    );
}

#[test]
fn form_urlencode_follows_whatwg_rules() {
    assert_eq!(form_urlencode("a b&c=d"), "a+b%26c%3Dd");
    assert_eq!(form_urlencode("a*b-c.d_e"), "a*b-c.d_e");
    assert_eq!(form_urlencode("caf\u{e9}"), "caf%C3%A9");
    assert_eq!(form_urlencode("100%"), "100%25");
}

#[test]
fn form_urlencoded_skips_unsuccessful_controls() {
    // #1538: only successful controls contribute to the submitted body.
    let mut text = named("user", FormControlKind::TextInput, "alice");
    let mut checked = named("remember", FormControlKind::Checkbox, "1");
    checked.checked = true;
    let unchecked = named("news", FormControlKind::Checkbox, "1");
    let mut disabled = named("secret", FormControlKind::TextInput, "x");
    disabled.disabled = true;
    let unnamed = named("", FormControlKind::TextInput, "ignored");
    let submit = named("go", FormControlKind::Submit, "Go");
    let controls = [text.clone(), checked, unchecked, disabled, unnamed, submit];
    text.name.clear(); // sanity: mutation isolated
    let body = form_urlencoded(&controls);
    assert_eq!(body, "user=alice&remember=1", "body={body}");
}

#[test]
fn form_urlencoded_checkbox_default_and_select() {
    let mut checkbox = named("ok", FormControlKind::Checkbox, "");
    checkbox.checked = true;
    let mut select = named("color", FormControlKind::Select, "");
    select.options = vec![
        SelectOption::new("Red", "r", true),
        SelectOption::new("Blue", "b", false),
    ];
    let body = form_urlencoded(&[checkbox, select]);
    assert_eq!(body, "ok=on&color=r", "body={body}");
}

#[test]
fn ime_state_tracks_composition_lifecycle() {
    // #1519: preedit accumulates composing text and commit returns the final text.
    let mut ime = ImeState::new();
    assert!(!ime.is_enabled());
    ime.enable();
    assert!(ime.is_enabled());
    ime.set_preedit("にほん".to_owned());
    assert_eq!(ime.preedit(), "にほん");
    let committed = ime.commit("日本語".to_owned());
    assert_eq!(committed, "日本語");
    assert!(ime.preedit().is_empty(), "preedit cleared after commit");
    ime.set_preedit("x".to_owned());
    assert_eq!(ime.disable(), "x", "disable returns pending preedit");
    assert!(!ime.is_enabled());
    assert!(ime.preedit().is_empty());
}

#[test]
fn control_kind_mapping_covers_the_ua_types() {
    assert_eq!(
        FormControlKind::from_element("input", Some("text")),
        Some(FormControlKind::TextInput)
    );
    assert_eq!(
        FormControlKind::from_element("input", Some("CHECKBOX")),
        Some(FormControlKind::Checkbox)
    );
    assert_eq!(
        FormControlKind::from_element("input", None),
        Some(FormControlKind::TextInput)
    );
    assert_eq!(
        FormControlKind::from_element("input", Some("submit")),
        Some(FormControlKind::Submit)
    );
    assert_eq!(
        FormControlKind::from_element("textarea", None),
        Some(FormControlKind::Textarea)
    );
    assert_eq!(
        FormControlKind::from_element("select", None),
        Some(FormControlKind::Select)
    );
    assert_eq!(
        FormControlKind::from_element("button", None),
        Some(FormControlKind::Button)
    );
    assert_eq!(FormControlKind::from_element("div", None), None);
    assert!(FormControlKind::TextInput.is_text_editable());
    assert!(!FormControlKind::Select.is_text_editable());
    assert!(FormControlKind::Radio.is_checkable());
    assert!(FormControlKind::Submit.is_button());
}

#[test]
fn focus_state_cycles_and_wraps() {
    let mut focus = FocusState::new();
    assert_eq!(focus.focused(), None);
    assert!(!focus.next(0));
    assert!(focus.next(3));
    assert_eq!(focus.focused(), Some(0));
    assert!(focus.next(3));
    assert_eq!(focus.focused(), Some(1));
    focus.focus(2, 3);
    assert!(focus.next(3));
    assert_eq!(focus.focused(), Some(0), "wraps to the first control");
    assert!(focus.previous(3));
    assert_eq!(focus.focused(), Some(2), "wraps to the last control");
    assert!(!focus.focus(3, 3), "out of range is rejected");
    focus.blur();
    assert_eq!(focus.focused(), None);
}

#[test]
fn text_edit_inserts_and_backspaces_around_the_caret() {
    let mut edit = TextEdit::new("ac");
    assert_eq!(edit.caret(), 2);
    assert!(edit.move_caret(false));
    assert!(edit.insert("b"));
    assert_eq!(edit.value(), "abc");
    assert_eq!(edit.caret(), 2);
    assert!(edit.backspace());
    assert_eq!(edit.value(), "ac");
    assert!(edit.move_caret(true));
    assert!(!edit.move_caret(true));
    // Only the ASCII-decodable characters have byte length 1; UTF-8 is handled by chars.
    let mut unicode = TextEdit::new("ñ");
    assert!(unicode.move_caret(false));
    assert!(unicode.insert("a"));
    assert_eq!(unicode.value(), "añ");
}

#[test]
fn select_options_are_extracted_and_patchable() {
    let output = crate::headless::render_target(
        "data:text/html,<form><select id=s><option value=a>One</option>             <option value=b selected>Two</option><option>Three</option></select></form>",
        240,
        160,
    )
    .expect("render");
    let mut document = output.document.clone();
    assign_form_ids(&mut document);
    let controls = collect_controls(&document, &output.layout);
    let select = controls
        .iter()
        .find(|control| control.kind == FormControlKind::Select)
        .expect("select control");
    assert_eq!(select.options.len(), 3, "options: {:?}", select.options);
    assert_eq!(select.options[0].label, "One");
    assert_eq!(select.options[0].value, "a");
    assert!(select.options[1].selected);
    assert_eq!(select.value, "Two");
    let select_id = select.form_id;
    assert!(patch_select(&mut document, select_id, 2));
    let patched = collect_controls(&document, &output.layout);
    let patched_select = patched
        .iter()
        .find(|control| control.kind == FormControlKind::Select)
        .expect("select");
    assert!(patched_select.options[2].selected);
    assert!(!patched_select.options[1].selected);
    assert_eq!(patched_select.value, "Three");
    assert!(!patch_select(&mut document, select_id, 9), "out of range");
    assert!(!patch_select(&mut document, 99, 0), "unknown control");
}

#[test]
fn select_optgroup_labels_are_extracted_for_the_dropdown() {
    let output = crate::headless::render_target(
        "data:text/html,<form><select><optgroup label=\"Fruits\">             <option>Apple</option><option>Pear</option></optgroup>             <optgroup label=\"Veg\"><option>Leek</option></optgroup></select></form>",
        240,
        160,
    )
    .expect("render");
    let mut document = output.document.clone();
    assign_form_ids(&mut document);
    let controls = collect_controls(&document, &output.layout);
    let select = controls
        .iter()
        .find(|control| control.kind == FormControlKind::Select)
        .expect("select");
    assert_eq!(select.options.len(), 3);
    assert_eq!(select.options[0].group.as_deref(), Some("Fruits"));
    assert_eq!(select.options[1].group.as_deref(), Some("Fruits"));
    assert_eq!(select.options[2].group.as_deref(), Some("Veg"));
}

#[test]
fn control_at_picks_the_topmost_control() {
    let mut first = control(0, FormControlKind::TextInput, "");
    first.rect = CssRect::new(0.0, 0.0, 100.0, 20.0);
    let mut second = control(1, FormControlKind::Button, "Go");
    second.rect = CssRect::new(10.0, 5.0, 30.0, 10.0);
    let controls = vec![first, second];
    assert_eq!(control_at(&controls, 15.0, 8.0), Some(1));
    assert_eq!(control_at(&controls, 80.0, 8.0), Some(0));
    assert_eq!(control_at(&controls, 200.0, 8.0), None);
}

#[test]
fn extraction_and_patching_roundtrip_through_a_rendered_page() {
    let output = crate::headless::render_target(
        "data:text/html,<form><input type=text value=hi><input type=checkbox>\
         <textarea>txt</textarea><button>Go</button></form>",
        200,
        120,
    )
    .expect("render");
    let mut document = output.document.clone();
    assign_form_ids(&mut document);
    let controls = collect_controls(&document, &output.layout);
    assert_eq!(controls.len(), 4, "controls: {controls:?}");
    assert_eq!(controls[0].kind, FormControlKind::TextInput);
    assert_eq!(controls[0].value, "hi");
    assert_eq!(controls[1].kind, FormControlKind::Checkbox);
    assert_eq!(controls[2].kind, FormControlKind::Textarea);
    assert_eq!(controls[2].value, "txt");
    assert_eq!(controls[3].kind, FormControlKind::Button);
    let text_id = controls[0].form_id;
    assert!(patch_control(
        &mut document,
        text_id,
        Some("edited"),
        None,
        true,
        false
    ));
    let patched = collect_controls(&document, &output.layout);
    assert_eq!(patched[0].value, "edited");
    assert!(document.get_elements_by_tag_name("input").iter().any(|id| {
        document
            .node(*id)
            .as_element()
            .is_some_and(|element| element.get_attribute(FOCUS_ATTR).is_some())
    }));
    // Checkbox toggle through the DOM attribute.
    let checkbox_id = controls[1].form_id;
    assert!(patch_control(
        &mut document,
        checkbox_id,
        None,
        Some(true),
        false,
        false
    ));
    let patched = collect_controls(&document, &output.layout);
    assert!(patched[1].checked);
    let url = snapshot_data_url(&document);
    assert!(url.starts_with("data:text/html;base64,"));
    assert!(!url.contains("<form"), "base64 payload is not plain text");
}

#[test]
fn multi_select_extraction_and_click_semantics() {
    let mut document = megi_webview::html::parse_document(
        "<select multiple data-megi-form-id=\"7\"><option>One</option><option selected>Two</option><option>Three</option><option>Four</option></select>",
        true,
    );
    crate::forms::assign_form_ids(&mut document);
    let options = vec![
        SelectOption::new("One", "1", false),
        SelectOption::new("Two", "2", true),
        SelectOption::new("Three", "3", false),
        SelectOption::new("Four", "4", false),
    ];
    let current = vec![1usize];
    assert_eq!(
        super::multi_click(&options, &current, Some(1), 3, false, true),
        vec![1, 2, 3]
    );
    assert_eq!(
        super::multi_click(&options, &current, Some(1), 1, true, false),
        Vec::<usize>::new()
    );
    assert_eq!(
        super::multi_click(&options, &current, Some(1), 0, true, false),
        vec![0, 1]
    );
    assert_eq!(
        super::multi_click(&options, &current, Some(1), 2, false, false),
        vec![2]
    );
    let control = FormControl {
        form_id: 7,
        name: String::new(),
        node_id: document.body().expect("body"),
        kind: FormControlKind::Select,
        rect: CssRect::new(0.0, 0.0, 100.0, 80.0),
        value: "Two".to_string(),
        checked: false,
        disabled: false,
        options: options.clone(),
        multiple: true,
    };
    assert_eq!(super::multi_option_at(&control, 25.0), Some(1));
    assert_eq!(super::multi_option_at(&control, 200.0), None);
    assert!(super::patch_multi_select(&mut document, 7, &[0, 2]));
    let text = megi_webview::html::serialize_html(&document);
    assert!(text.contains("selected"));
}

#[test]
fn multi_select_visual_state_reaches_the_dom() {
    let mut document = megi_webview::html::parse_document(
        "<select multiple data-megi-form-id=\"3\"><option>A</option><option>B</option><option>C</option></select>",
        true,
    );
    crate::forms::assign_form_ids(&mut document);
    assert!(super::patch_multi_select(&mut document, 3, &[2]));
    let serialized = megi_webview::html::serialize_html(&document);
    assert!(serialized.contains("selected"), "{serialized}");
    let options = super::select_options(&document, document.body().expect("body"));
    assert!(!options.is_empty());
}

#[test]
fn widget_visual_covers_native_states() {
    let normal = widget_visual(FormControlKind::Button, false, false, false, false, false);
    let hovered = widget_visual(FormControlKind::Button, false, false, true, false, false);
    let pressed = widget_visual(FormControlKind::Button, false, false, false, true, false);
    let disabled = widget_visual(FormControlKind::Button, false, false, false, false, true);
    assert_ne!(normal.background, hovered.background);
    assert_ne!(normal.background, pressed.background);
    assert_eq!(disabled.border, [0xB8, 0xBF, 0xC7]);
    let checked = widget_visual(FormControlKind::Checkbox, true, true, false, false, false);
    assert_eq!(checked.background, [0x1F, 0x6F, 0xEB]);
    assert!(checked.focus_ring);
    let focused_disabled = widget_visual(FormControlKind::Radio, true, true, false, false, true);
    assert!(!focused_disabled.focus_ring);
}

#[test]
fn caret_blink_is_deterministic() {
    assert!(caret_visible(0));
    assert!(caret_visible(10));
    assert!(!caret_visible(11));
    assert!(!caret_visible(20));
    assert!(caret_visible(21));
    // Same logical tick always yields the same visibility (no wall clock).
    for tick in 0..100u64 {
        assert_eq!(caret_visible(tick), caret_visible(tick + 21));
    }
}
#[test]
fn form_ids_survive_hostile_page_values() {
    // `u32::MAX` used to panic on `id + 1`; duplicates and duplicates of the counter
    // made several controls share an id.
    let mut document = megi_webview::html::parse_document(
        "<body><input data-megi-form-id='4294967295'>\
         <input data-megi-form-id='4294967294'>\
         <input data-megi-form-id='7'><input data-megi-form-id='7'>\
         <input></body>",
        false,
    );
    let next = assign_form_ids(&mut document);
    let mut ids = Vec::new();
    for node_id in element_ids(&document) {
        if let Some(element) = document.node(node_id).as_element()
            && let Some(value) = element.get_attribute(FORM_ID_ATTR)
        {
            ids.push(value.parse::<u32>().expect("numeric form id"));
        }
    }
    assert_eq!(ids.len(), 5, "every control gets an id: {ids:?}");
    let unique: std::collections::HashSet<u32> = ids.iter().copied().collect();
    assert_eq!(unique.len(), 5, "ids are unique: {ids:?}");
    assert!(next > 0);
}

#[test]
fn ime_commit_ends_composition_so_keys_insert_once() {
    // #1722: committing (the `Ime::Commit` path uses commit(), not commit_events())
    // must end composition, otherwise the KeyboardInput guard swallows later keys.
    let mut ime = ImeState::new();
    ime.enable();
    ime.preedit_events("に".to_owned());
    assert!(ime.is_composing());
    assert_eq!(ime.commit("にほん".to_owned()), "にほん");
    assert!(!ime.is_composing(), "commit() must end composition (#1722)");
    ime.preedit_events("x".to_owned());
    assert!(ime.is_composing());
    ime.commit_events("x".to_owned());
    assert!(!ime.is_composing(), "commit_events() must end composition");
    ime.preedit_events("y".to_owned());
    ime.disable();
    assert!(!ime.is_composing(), "disable() must cancel composition");
}

#[test]
fn ime_state_emits_composition_event_sequence() {
    // F7-IME: DOM compositionstart/update/end map to the OS IME lifecycle.
    let mut ime = ImeState::new();
    ime.enable();
    let first = ime.preedit_events("に".to_owned());
    assert_eq!(
        first
            .iter()
            .map(|event| event.event_type())
            .collect::<Vec<_>>(),
        vec!["compositionstart", "compositionupdate"]
    );
    let second = ime.preedit_events("にほん".to_owned());
    assert_eq!(
        second
            .iter()
            .map(|event| event.event_type())
            .collect::<Vec<_>>(),
        vec!["compositionupdate"]
    );
    let end = ime.commit_events("日本語".to_owned());
    assert_eq!(
        end.iter()
            .map(|event| event.event_type())
            .collect::<Vec<_>>(),
        vec!["compositionend"]
    );
    assert_eq!(end[0].data(), "日本語");
}
