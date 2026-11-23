//! Unit tests for O(1), zero-clone live form editing and debounced JS event coalescing (#2199).

use std::sync::Arc;
use std::time::{Duration, Instant};

use megi_webview::core::{Attribute, Document, Namespace};
use megi_webview::render::{HostDomEvent, HostEventTarget, HostUiEventKind};

use crate::forms::{
    FOCUS_ATTR, FORM_ID_ATTR, FormControl, FormControlKind, LiveFormPatcher, LivePatchInput,
    PatchOutcome, SelectOption, control_display_text,
};
use crate::links::CssRect;
use crate::worker::{FrameStore, RenderFrame, WorkerMessage};

#[test]
fn live_form_typing_avoids_dom_clones_and_renders_across_60_keystrokes() {
    let mut doc = Document::new();
    let body = doc.create_element(Namespace::Html, "body", Vec::new());
    assert!(doc.append_child(doc.root(), body));
    for _ in 0..5_000 {
        let p = doc.create_element(Namespace::Html, "p", Vec::new());
        let txt = doc.create_text_node("filler");
        assert!(doc.append_child(p, txt));
        assert!(doc.append_child(body, p));
    }
    let input_node = doc.create_element(
        Namespace::Html,
        "input",
        vec![
            Attribute::new("type", "text"),
            Attribute::new(FORM_ID_ATTR, "7"),
        ],
    );
    assert!(doc.append_child(body, input_node));

    let frame = RenderFrame {
        generation: 1,
        width: 200,
        height: 100,
        pixels: vec![0; 200 * 100],
        png_bytes: Vec::new(),
        content_height: 100,
        capped: false,
        links: Vec::new(),
        images: Vec::new(),
        title: None,
        document_url: "https://example.test/form".to_owned(),
        scripts_executed: 0,
        script_timers_pending: 0,
        warnings: 0,
        script_errors: 0,
        forms: vec![FormControl {
            form_id: 7,
            name: "q".to_owned(),
            node_id: input_node,
            kind: FormControlKind::TextInput,
            rect: CssRect::new(10.0, 10.0, 120.0, 24.0),
            value: String::new(),
            checked: false,
            disabled: false,
            options: Vec::new(),
            multiple: false,
        }],
        document: Arc::new(doc),
        find_matches: Vec::new(),
        find_rects: Vec::new(),
    };

    let mut store = FrameStore::new();
    let _ = store.begin_request("https://example.test/form", 200, 100);
    assert!(store.accept(WorkerMessage::Frame(frame)));
    let mut doc_arc = store
        .take_frame_document()
        .expect("window must take unique document ownership");
    assert_eq!(Arc::strong_count(&doc_arc), 1);
    let initial_ptr = Arc::as_ptr(&doc_arc);

    let mut forms = vec![store.frame().expect("frame").forms[0].clone()];
    let mut patcher = LiveFormPatcher::new();
    let mut full_renders = 0usize;
    let mut typed = String::new();
    let base_time = Instant::now();

    for i in 0..60usize {
        typed.push((b'a' + (i % 26) as u8) as char);
        let outcome = patcher.apply_patch(
            &mut doc_arc,
            &mut forms,
            LivePatchInput {
                target_form: Some(7),
                focused_link: None,
                value: Some(&typed),
                checked: None,
                focused: true,
                active: false,
                scripts_active: false,
                ui_events: Vec::new(),
                now: base_time + Duration::from_millis((i as u64) * 20),
            },
        );
        assert_eq!(outcome, PatchOutcome::LocalOnly);
        assert_eq!(
            Arc::as_ptr(&doc_arc),
            initial_ptr,
            "keystroke {i} must mutate Document in-place without deep-cloning"
        );
    }
    if patcher
        .flush_if_due(base_time + Duration::from_secs(5))
        .is_some()
    {
        full_renders += 1;
    }
    assert_eq!(
        full_renders, 0,
        "non-JS form edits must emit 0 full renders"
    );
    assert_eq!(forms[0].value, typed);
    assert_eq!(
        doc_arc
            .node(input_node)
            .as_element()
            .expect("input element")
            .get_attribute("value"),
        Some(typed.as_str())
    );
    assert_eq!(patcher.dirty_forms(), &[7]);
}

#[test]
fn live_form_typing_coalesces_js_events_without_cloning_dom() {
    let mut doc = Document::new();
    let body = doc.create_element(Namespace::Html, "body", Vec::new());
    let input_node = doc.create_element(
        Namespace::Html,
        "input",
        vec![Attribute::new(FORM_ID_ATTR, "3")],
    );
    assert!(doc.append_child(doc.root(), body));
    assert!(doc.append_child(body, input_node));
    let mut doc_arc = Arc::new(doc);
    let initial_ptr = Arc::as_ptr(&doc_arc);

    let mut forms = vec![FormControl {
        form_id: 3,
        name: "k".to_owned(),
        node_id: input_node,
        kind: FormControlKind::TextInput,
        rect: CssRect::new(0.0, 0.0, 100.0, 20.0),
        value: String::new(),
        checked: false,
        disabled: false,
        options: Vec::new(),
        multiple: false,
    }];
    let mut patcher = LiveFormPatcher::new();
    let start = Instant::now();
    let mut val = String::new();
    for i in 0..10u64 {
        val.push('x');
        let ev = HostDomEvent {
            event_type: "input".to_owned(),
            target: HostEventTarget::FormControl(3),
            bubbles: true,
            cancelable: false,
            kind: HostUiEventKind::Input {
                data: Some("x".to_owned()),
                input_type: "insertText".to_owned(),
            },
        };
        let outcome = patcher.apply_patch(
            &mut doc_arc,
            &mut forms,
            LivePatchInput {
                target_form: Some(3),
                focused_link: None,
                value: Some(&val),
                checked: None,
                focused: true,
                active: false,
                scripts_active: true,
                ui_events: vec![ev],
                now: start + Duration::from_millis(i * 10),
            },
        );
        assert!(matches!(outcome, PatchOutcome::QueuedJsFlush { .. }));
    }
    assert_eq!(Arc::as_ptr(&doc_arc), initial_ptr);
    assert!(
        patcher
            .flush_if_due(start + Duration::from_millis(95))
            .is_none()
    );
    let flushed = patcher
        .flush_if_due(start + Duration::from_millis(250))
        .expect("debounced JS flush must fire once after quiet window");
    assert_eq!(flushed.len(), 10);
}

#[test]
fn live_select_and_focus_blur_patch_in_place_without_dom_clones() {
    let mut doc = Document::new();
    let body = doc.create_element(Namespace::Html, "body", Vec::new());
    let input_a = doc.create_element(
        Namespace::Html,
        "input",
        vec![Attribute::new(FORM_ID_ATTR, "1")],
    );
    let select = doc.create_element(
        Namespace::Html,
        "select",
        vec![Attribute::new(FORM_ID_ATTR, "2")],
    );
    let opt0 = doc.create_element(
        Namespace::Html,
        "option",
        vec![Attribute::new("value", "a"), Attribute::new("selected", "")],
    );
    let opt1 = doc.create_element(
        Namespace::Html,
        "option",
        vec![Attribute::new("value", "b")],
    );
    assert!(doc.append_child(doc.root(), body));
    assert!(doc.append_child(body, input_a));
    assert!(doc.append_child(body, select));
    assert!(doc.append_child(select, opt0));
    assert!(doc.append_child(select, opt1));

    let mut doc_arc = Arc::new(doc);
    let ptr = Arc::as_ptr(&doc_arc);
    let mut forms = vec![
        FormControl {
            form_id: 1,
            name: "a".to_owned(),
            node_id: input_a,
            kind: FormControlKind::TextInput,
            rect: CssRect::new(0.0, 0.0, 80.0, 20.0),
            value: String::new(),
            checked: false,
            disabled: false,
            options: Vec::new(),
            multiple: false,
        },
        FormControl {
            form_id: 2,
            name: "s".to_owned(),
            node_id: select,
            kind: FormControlKind::Select,
            rect: CssRect::new(0.0, 30.0, 80.0, 20.0),
            value: "a".to_owned(),
            checked: false,
            disabled: false,
            options: vec![
                SelectOption {
                    label: "Alpha".to_owned(),
                    value: "a".to_owned(),
                    selected: true,
                    group: None,
                },
                SelectOption {
                    label: "Beta".to_owned(),
                    value: "b".to_owned(),
                    selected: false,
                    group: None,
                },
            ],
            multiple: false,
        },
    ];

    let mut patcher = LiveFormPatcher::new();
    let now = Instant::now();
    let _ = patcher.apply_patch(
        &mut doc_arc,
        &mut forms,
        LivePatchInput {
            target_form: Some(1),
            focused_link: None,
            value: Some("hi"),
            checked: None,
            focused: true,
            active: false,
            scripts_active: false,
            ui_events: Vec::new(),
            now,
        },
    );
    let _ = patcher.apply_patch(
        &mut doc_arc,
        &mut forms,
        LivePatchInput {
            target_form: Some(2),
            focused_link: None,
            value: None,
            checked: None,
            focused: true,
            active: false,
            scripts_active: false,
            ui_events: Vec::new(),
            now,
        },
    );
    assert_eq!(Arc::as_ptr(&doc_arc), ptr);
    assert!(
        doc_arc
            .node(input_a)
            .as_element()
            .expect("input_a")
            .get_attribute(FOCUS_ATTR)
            .is_none()
    );
    assert!(
        doc_arc
            .node(select)
            .as_element()
            .expect("select")
            .get_attribute(FOCUS_ATTR)
            .is_some()
    );

    assert!(patcher.apply_select(&mut doc_arc, &mut forms, 2, 1));
    assert_eq!(Arc::as_ptr(&doc_arc), ptr);
    assert_eq!(forms[1].value, "b");
    assert_eq!(control_display_text(&forms[1]), "Beta");
    assert!(
        doc_arc
            .node(opt0)
            .as_element()
            .expect("opt0")
            .get_attribute("selected")
            .is_none()
    );
    assert!(
        doc_arc
            .node(opt1)
            .as_element()
            .expect("opt1")
            .get_attribute("selected")
            .is_some()
    );
}
