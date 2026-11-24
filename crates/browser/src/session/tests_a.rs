#[test]
fn temporary_file_guard_removes_partial_files() {
    let dir = std::env::temp_dir().join(format!("megi-tmp-guard-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("session.json.tmp1");
    std::fs::write(&path, b"partial").expect("create temp");

    {
        let guard = TemporaryFileGuard::new(path.clone());
        drop(guard);
    }
    assert!(
        !path.exists(),
        "a failed write must not leave a .tmp file (#1462)"
    );

    std::fs::write(&path, b"complete").expect("create temp 2");
    {
        let guard = TemporaryFileGuard::new(path.clone());
        guard.keep();
    }
    assert!(
        path.exists(),
        "a successful write keeps the file for rename"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

use super::*;

pub(crate) fn sample() -> Session {
    Session {
        version: SCHEMA_VERSION,
        active: 1,
        tabs: vec![
            SessionTab {
                url: "data:text/html,<h1>one</h1>".to_string(),
                history: vec!["data:text/html,<h1>one</h1>".to_string()],
                index: 0,
                zoom: 1.0,
                scroll: 0,
            },
            SessionTab {
                url: "file:///tmp/two.html".to_string(),
                history: vec![
                    "file:///tmp/one.html".to_string(),
                    "file:///tmp/two.html".to_string(),
                ],
                index: 1,
                zoom: 1.5,
                scroll: 120,
            },
        ],
        recent: Vec::new(),
        site_zooms: Vec::new(),
        reader_html: None,
    }
}

#[test]
fn json_roundtrip_is_deterministic() {
    let session = sample();
    let json = session.to_json();
    let parsed = Session::from_json(&json).expect("parse");
    assert_eq!(parsed, session);
    assert_eq!(parsed.to_json(), json, "writer is deterministic");
}

#[test]
fn json_roundtrip_preserves_reader_html_1725() {
    let mut session = sample();
    session.reader_html = Some(
        "data:text/html,%3Ctitle%3EReader%3C%2Ftitle%3E%3Ch1%3EArticle%3C%2Fh1%3E".to_string(),
    );
    let json = session.to_json();
    let parsed = Session::from_json(&json).expect("parse");
    assert_eq!(parsed.reader_html, session.reader_html);
    assert_eq!(parsed.to_json(), json, "writer is deterministic");
}

#[test]
fn old_session_without_reader_html_parses_1725() {
    let mut session = sample();
    session.reader_html = None;
    let json = session.to_json();
    assert!(
        !json.contains("readerHtml"),
        "optional key omitted when absent"
    );
    let parsed = Session::from_json(&json).expect("parse");
    assert_eq!(parsed.reader_html, None);
}

#[test]
fn non_data_reader_html_is_rejected_1725() {
    let mut session = sample();
    session.reader_html = Some("javascript:alert(1)".to_string());
    let json = session.to_json();
    let parsed = Session::from_json(&json).expect("parse");
    assert_eq!(
        parsed.reader_html, None,
        "only data:text/html URLs are accepted"
    );
}

#[test]
fn escapes_strings_and_rejects_trailing_garbage() {
    let mut session = sample();
    session.tabs[0].url = "data:text/html,<p>quote\" slash\\ tab\t</p>".to_string();
    let json = session.to_json();
    let parsed = Session::from_json(&json).expect("parse");
    assert_eq!(parsed.tabs[0].url, session.tabs[0].url);
    assert!(Session::from_json("{} trailing").is_err());
    assert!(Session::from_json("{").is_err());
    assert!(Session::from_json("{\"version\":").is_err());
}

#[test]
fn corrupt_and_unsupported_versions_are_tolerated_at_load() {
    let loaded = load(Path::new("/definitely/missing/session.json"));
    assert!(loaded.session.is_none());
    assert!(loaded.warning.is_none(), "missing file is a fresh session");

    let mut session = sample();
    session.version = 99;
    assert!(Session::from_json(&session.to_json()).is_err());
    assert!(Session::from_json("not json").is_err());
    assert!(Session::from_json("{\"version\":1}").is_err());
}

#[test]
fn normalization_caps_history_tabs_and_clamps_values() {
    let mut tabs = Vec::new();
    for index in 0..(MAX_SESSION_TABS + 5) {
        let url = format!("data:text/html,{index}");
        tabs.push(SessionTab {
            url: url.clone(),
            history: (0..(MAX_HISTORY_PER_TAB + 10))
                .map(|item| format!("data:text/html,{index}-{item}"))
                .collect(),
            index: 500,
            zoom: 99.0,
            scroll: 7,
        });
    }
    let mut session = Session {
        version: SCHEMA_VERSION,
        active: 999,
        tabs,
        recent: Vec::new(),
        site_zooms: Vec::new(),
        reader_html: None,
    };
    session.normalize();
    assert_eq!(session.tabs.len(), MAX_SESSION_TABS);
    assert_eq!(session.tabs[0].history.len(), MAX_HISTORY_PER_TAB);
    assert_eq!(session.tabs[0].zoom, 3.0);
    assert_eq!(session.tabs[0].index, MAX_HISTORY_PER_TAB - 1);
    assert_eq!(session.active, MAX_SESSION_TABS - 1);
}

#[test]
fn capture_and_restore_keep_tabs_zoom_scroll_and_history() {
    let mut tabs = TabsState::from_urls(vec![
        "data:text/html,one".to_string(),
        "file:///tmp/two.html".to_string(),
    ]);
    assert!(tabs.activate(1));
    {
        let tab = tabs.active_mut();
        tab.zoom = 1.5;
        tab.scroll = ScrollState::new(200, 100);
        tab.scroll.scroll_to(80);
        tab.navigation
            .navigate_to(megi_webview::net::Url::parse("file:///tmp/three.html").expect("url"));
        tab.url = "file:///tmp/three.html".to_string();
    }
    let session = Session::capture(&tabs);
    assert_eq!(session.tabs.len(), 2);
    assert_eq!(session.active, 1);
    assert_eq!(session.tabs[1].zoom, 1.5);
    assert_eq!(session.tabs[1].scroll, 80);
    assert_eq!(session.tabs[1].history.len(), 2);

    let restored = session.restore();
    assert_eq!(restored.len(), 2);
    assert_eq!(restored.active_index(), 1);
    assert_eq!(restored.active().url, "file:///tmp/three.html");
    assert!((restored.active().zoom - 1.5).abs() < f64::EPSILON);
    assert_eq!(restored.active().navigation.history().len(), 2);
    assert_eq!(restored.active().navigation.index(), 1);
    assert!(restored.active().scroll.offset() > 0);
}

#[test]
fn save_and_load_roundtrip_in_a_temp_directory() {
    let dir = std::env::temp_dir().join(format!(
        "megi-session-test-{}-{}",
        std::process::id(),
        line!()
    ));
    let _ = fs::remove_dir_all(&dir);
    let path = dir.join("session.json");
    save(&path, &sample()).expect("save");
    let loaded = load(&path);
    assert!(loaded.warning.is_none());
    assert_eq!(loaded.session, Some(sample()));
    // Corrupt the file: load returns a warning and no session.
    fs::write(&path, "{broken").expect("write corrupt");
    let corrupt = load(&path);
    assert!(corrupt.session.is_none());
    assert!(corrupt.warning.is_some());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn empty_session_restores_a_blank_tab() {
    let restored = Session::empty().restore();
    assert_eq!(restored.len(), 1);
    assert_eq!(restored.active().url, crate::tabs::BLANK_TAB_URL);
    assert!(Session::empty().to_json().contains("\"tabs\":[]"));
}

#[test]
fn unknown_fields_are_ignored_and_numbers_are_clamped() {
    let json = "{\"version\":1,\"active\":5,\"future\":{\"x\":[1,2]},\"tabs\":[{\"url\":\"data:text/html,x\",\"history\":[\"data:text/html,x\"],\"index\":9,\"zoom\":99,\"scroll\":3,\"unknown\":true}]}";
    let session = Session::from_json(json).expect("tolerant parse");
    assert_eq!(session.tabs.len(), 1);
    assert!(session.tabs[0].index < session.tabs[0].history.len());
    assert!(session.tabs[0].zoom <= 3.0);
    assert_eq!(session.active, 0);
}

#[test]
fn default_paths_follow_xdg_then_home() {
    let xdg = Path::new("/xdg/data");
    let home = Path::new("/home/user");
    assert_eq!(
        default_session_path(Some(xdg), Some(home)),
        Some(PathBuf::from("/xdg/data/megi/session.json"))
    );
    assert_eq!(
        default_session_path(None, Some(home)),
        Some(PathBuf::from("/home/user/.local/share/megi/session.json"))
    );
    assert_eq!(default_session_path(None, None), None);
}

#[test]
fn lock_is_best_effort_and_released_on_drop() {
    let dir = std::env::temp_dir().join(format!(
        "megi-session-lock-{}-{}",
        std::process::id(),
        line!()
    ));
    let _ = fs::remove_dir_all(&dir);
    let path = dir.join("session.json");
    let first = SessionLock::acquire(&path).expect("acquire").expect("free");
    assert!(first.path().is_file());
    assert!(
        SessionLock::acquire(&path).expect("acquire").is_none(),
        "second acquisition fails while locked"
    );
    drop(first);
    assert!(SessionLock::acquire(&path).expect("acquire").is_some());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn recent_history_roundtrips_through_the_session() {
    let mut session = sample();
    session.recent = vec![crate::history::HistoryEntry {
        title: "Example".to_string(),
        url: "https://example.com/".to_string(),
        logical_time: 4,
    }];
    let parsed = Session::from_json(&session.to_json()).expect("parse");
    assert_eq!(parsed.recent, session.recent);
    assert!(
        Session::from_json(&sample().to_json())
            .expect("old files")
            .recent
            .is_empty()
    );
}

#[test]
fn site_zooms_roundtrip_tolerantly() {
    let mut session = sample();
    session.site_zooms = vec![
        ("https://example.com".to_string(), 1.5),
        ("file://".to_string(), 0.75),
    ];
    let parsed = Session::from_json(&session.to_json()).expect("parse");
    assert_eq!(parsed.site_zooms, session.site_zooms);
    let tolerant = Session::from_json(
        "{\"version\":1,\"active\":0,\"tabs\":[],\"siteZooms\":[[\"https://a.example\",9.0],\"bad\",[\"\",1.0]]}",
    )
    .expect("tolerant");
    assert_eq!(
        tolerant.site_zooms,
        vec![("https://a.example".to_string(), 3.0)]
    );
    assert!(
        Session::from_json("{\"version\":1,\"active\":0,\"tabs\":[]}")
            .expect("old")
            .site_zooms
            .is_empty()
    );
}

#[test]
fn persisted_recent_history_keeps_titles_and_domains() {
    let mut session = sample();
    session.recent = vec![
        crate::history::HistoryEntry {
            title: "Home".to_string(),
            url: "https://example.com/home".to_string(),
            logical_time: 1,
        },
        crate::history::HistoryEntry {
            title: "Docs".to_string(),
            url: "https://docs.example/start".to_string(),
            logical_time: 2,
        },
    ];
    let parsed = Session::from_json(&session.to_json()).expect("parse");
    assert_eq!(parsed.recent, session.recent);
    assert_eq!(
        crate::history::domain_of(&parsed.recent[1].url),
        "docs.example"
    );
    assert!(
        crate::history::domain_badge(&parsed.recent[1].url)
            .0
            .is_ascii_uppercase()
    );
}
