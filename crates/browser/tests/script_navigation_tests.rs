//! #1828: location/history actions from page scripts reach the shell's RenderFrame.

use megi_browser::worker::{RenderRequest, render_document};

fn request(html: &str) -> RenderRequest {
    let data = format!("data:text/html,{}", urlencoding(html));
    RenderRequest {
        generation: 1,
        input: data,
        css_width: 400,
        css_height: 300,
        base_override: None,
        scripting: true,
        find_query: None,
        host_events: Vec::new(),
        host_dom_events: Vec::new(),
        form_post: None,
    }
}

fn urlencoding(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            ' ' => "%20".into(),
            '<' => "%3C".into(),
            '>' => "%3E".into(),
            '"' => "%22".into(),
            '\'' => "%27".into(),
            ';' => "%3B".into(),
            '=' => "%3D".into(),
            '\n' => "%0A".into(),
            '#' => "%23".into(),
            '?' => "%3F".into(),
            '&' => "%26".into(),
            '/' => "%2F".into(),
            ':' => "%3A".into(),
            '(' => "%28".into(),
            ')' => "%29".into(),
            other => other.to_string(),
        })
        .collect()
}

#[test]
fn location_assign_action_is_propagated() {
    let html = "<script>location.href='https://example.org/next';</script>";
    let frame = render_document(&request(html)).expect("render");
    assert!(
        !frame.location_actions.is_empty(),
        "expected a location action in the frame"
    );
}

#[test]
fn history_push_state_action_is_propagated() {
    let html = "<script>history.pushState({}, '', '/next');</script>";
    let frame = render_document(&request(html)).expect("render");
    assert!(
        !frame.history_actions.is_empty(),
        "expected a history action in the frame"
    );
}
