use super::tests_a::sample;
use super::*;

#[test]
fn capture_remaps_the_history_index_and_keeps_the_current_entry() {
    let mut tabs = crate::tabs::TabsState::new("data:text/html,0");
    for index in 1..100 {
        let url = megi_webview::net::Url::parse(&format!("data:text/html,{index}")).expect("url");
        tabs.active_mut().navigation.navigate_to(url);
    }
    for _ in 0..90 {
        assert!(tabs.active_mut().navigation.back().is_some());
    }
    let current = tabs
        .active()
        .navigation
        .current()
        .expect("current")
        .as_str()
        .to_owned();
    let session = Session::capture(&tabs);
    let tab = &session.tabs[0];
    assert_eq!(tab.history.len(), MAX_HISTORY_PER_TAB);
    assert!(tab.index < tab.history.len());
    assert_eq!(
        tab.history[tab.index], current,
        "the saved index must point at the current URL"
    );
}

#[test]
fn normalize_remaps_the_index_after_dropping_invalid_urls() {
    let json = r#"{"version":1,"active":0,"tabs":[{"url":"https://a/","history":["not a url","https://a/","https://b/"],"index":1,"zoom":1.0,"scroll":0}]}"#;
    let session = Session::from_json(json).expect("parse");
    let tab = &session.tabs[0];
    assert_eq!(tab.index, 0);
    assert_eq!(tab.history[tab.index], "https://a/");
    let restored = session.restore();
    assert_eq!(
        restored
            .active()
            .navigation
            .current()
            .map(megi_webview::net::Url::as_str)
            .as_deref(),
        Some("https://a/")
    );
}

#[test]
fn json_roundtrip_preserves_non_ascii_strings() {
    let mut session = sample();
    session.recent.push(crate::history::HistoryEntry {
        title: "café".to_string(),
        url: "https://example.com/".to_string(),
        logical_time: 1,
    });
    let json = session.to_json();
    let parsed = Session::from_json(&json).expect("parse");
    assert_eq!(parsed.recent[0].title, "café");
    assert_eq!(parsed, session);
}

#[test]
fn oversized_session_files_are_rejected() {
    let path = std::env::temp_dir().join(format!("megi-session-big-{}.json", std::process::id()));
    let mut json = String::from("{\"version\":1,\"tabs\":[],\"pad\":\"");
    json.push_str(&"x".repeat(MAX_SESSION_BYTES as usize));
    json.push_str("\"}");
    std::fs::write(&path, json).expect("write");
    let loaded = load(&path);
    assert!(loaded.session.is_none());
    assert!(
        loaded
            .warning
            .as_deref()
            .is_some_and(|warning| warning.contains("larger than")),
        "{:?}",
        loaded.warning
    );
    let _ = std::fs::remove_file(&path);
}

#[cfg(unix)]
#[test]
fn special_files_are_read_with_the_cap() {
    if !Path::new("/dev/zero").exists() {
        return;
    }
    let loaded = load(Path::new("/dev/zero"));
    assert!(loaded.session.is_none());
    assert!(
        loaded
            .warning
            .as_deref()
            .is_some_and(|warning| warning.contains("larger than")),
        "{:?}",
        loaded.warning
    );
}
#[test]
fn capture_slides_the_window_when_the_current_entry_is_late() {
    let mut tabs = crate::tabs::TabsState::new("data:text/html,0");
    for index in 1..256 {
        let url = megi_webview::net::Url::parse(&format!("data:text/html,{index}")).expect("url");
        tabs.active_mut().navigation.navigate_to(url);
    }
    let current = tabs
        .active()
        .navigation
        .current()
        .expect("current")
        .as_str();
    let session = Session::capture(&tabs);
    let tab = &session.tabs[0];
    assert_eq!(tab.history.len(), MAX_HISTORY_PER_TAB);
    assert_eq!(tab.index, MAX_HISTORY_PER_TAB - 1);
    assert_eq!(tab.history[tab.index], current);
}

#[test]
fn normalize_moves_the_index_off_an_invalid_url() {
    let json = r#"{"version":1,"active":0,"tabs":[{"url":"https://b/","history":["https://a/","not a url","https://b/"],"index":1,"zoom":1.0,"scroll":0}]}"#;
    let session = Session::from_json(json).expect("parse");
    let tab = &session.tabs[0];
    assert_eq!(tab.history.len(), 2);
    assert_eq!(tab.index, 0, "the index falls back to the predecessor");
    assert_eq!(tab.history[tab.index], "https://a/");
}

#[test]
fn json_roundtrip_preserves_four_byte_scalars() {
    let mut session = sample();
    session.recent.push(crate::history::HistoryEntry {
        title: "emoji 🚀 fin".to_string(),
        url: "https://example.com/".to_string(),
        logical_time: 2,
    });
    let parsed = Session::from_json(&session.to_json()).expect("parse");
    assert_eq!(parsed.recent[0].title, "emoji 🚀 fin");
}

#[test]
fn saving_an_oversized_session_is_rejected_instead_of_written() {
    let dir = std::env::temp_dir().join(format!("megi-session-over-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("session.json");
    let mut session = sample();
    session.tabs[0].url = format!("data:text/html,{}", "x".repeat(MAX_SESSION_BYTES as usize));
    let error = save(&path, &session).expect_err("session exceeds the read cap");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(!path.exists(), "nothing is written");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn temporaries_are_unique_and_exclusive() {
    let dir = std::env::temp_dir().join(format!("megi-session-tmp-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("session.json");
    let first = write_temporary(&path, b"{}").expect("first temporary");
    let second = write_temporary(&path, b"{}").expect("second temporary");
    assert_ne!(first, second, "each call gets its own temporary");
    assert!(first.is_file() && second.is_file());
    let _ = std::fs::remove_file(&first);
    let _ = std::fs::remove_file(&second);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn failed_saves_clean_up_their_temporary() {
    let dir = std::env::temp_dir().join(format!("megi-session-fail-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    // Renaming onto a directory fails; the temporary must not be left behind.
    let target = dir.join("session.json");
    std::fs::create_dir(&target).expect("target dir");
    assert!(save(&target, &sample()).is_err());
    let leftovers: Vec<String> = std::fs::read_dir(&dir)
        .expect("read dir")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "no temporary left: {leftovers:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn saves_use_a_unique_temporary_and_leave_no_files() {
    let dir = std::env::temp_dir().join(format!("megi-session-save-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("session.json");
    let session = sample();
    save(&path, &session).expect("save");
    assert!(path.is_file(), "the session was renamed into place");
    let leftovers: Vec<String> = std::fs::read_dir(&dir)
        .expect("read dir")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".tmp"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "no temporary files left: {leftovers:?}"
    );
    let parsed = Session::from_json(&std::fs::read_to_string(&path).expect("read"))
        .expect("parse saved session");
    assert_eq!(parsed, session);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn normalize_and_restore_preserve_valid_tabs_across_corrupt_entries() {
    let mut session = sample();
    session.tabs.push(SessionTab {
        url: String::new(),
        history: vec![],
        index: 99,
        scroll: 0,
        zoom: f64::NAN,
    });
    session.tabs.push(SessionTab {
        url: "https://example.org/".to_string(),
        history: vec!["https://example.org/".to_string()],
        index: 0,
        scroll: 0,
        zoom: -5.0,
    });
    session.normalize();
    assert_eq!(session.tabs.len(), 4);
    assert_eq!(session.tabs[2].zoom, 1.0);
    assert_eq!(session.tabs[3].zoom, 1.0);

    let restored = session.restore();
    assert_eq!(restored.len(), 3);
    assert_eq!(restored.tabs()[2].url, "https://example.org/");
}

#[cfg(unix)]
#[test]
fn save_creates_private_directories_and_session_file() {
    use std::os::unix::fs::PermissionsExt;

    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let base =
        std::env::temp_dir().join(format!("megi-session-mode-{}-{unique}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).expect("base dir");
    let parent = base.join("private").join("nested");
    let path = parent.join("session.json");
    save(&path, &sample()).expect("save");

    for dir in [base.join("private"), parent.clone()] {
        let mode = std::fs::metadata(&dir)
            .expect("dir metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            mode,
            0o700,
            "session directories must be private: {}",
            dir.display()
        );
    }
    let mode = std::fs::metadata(&path)
        .expect("file metadata")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(
        mode, 0o600,
        "the session file must not be readable by others"
    );
    let _ = std::fs::remove_dir_all(&base);
}
