#![no_main]
//! Fuzz target: `file://` URL parsing and `FileGrantTable` containment checks.

use std::path::Path;
use std::sync::OnceLock;

use libfuzzer_sys::fuzz_target;
use megi_browser::ipc::{FileGrant, FileGrantTable};

fn grant_table() -> &'static (FileGrantTable, FileGrant) {
    static TABLE: OnceLock<(FileGrantTable, FileGrant)> = OnceLock::new();
    TABLE.get_or_init(|| {
        let root = std::env::temp_dir().join("megi-fuzz-file-grant");
        let _ = std::fs::create_dir_all(&root);
        let table = FileGrantTable::new();
        let grant = table.grant_for(&root).expect("create fuzz file grant root");
        (table, grant)
    })
}

fuzz_target!(|data: &[u8]| {
    if data.len() > 16 * 1024 {
        return;
    }
    let text = String::from_utf8_lossy(data);
    let _ = megi_net::Url::parse(&text);
    let _ = megi_net::Url::parse(&format!("file://{text}"));

    let (table, grant) = grant_table();
    let _ = table.resolve_within(grant, Path::new(text.as_ref()));
});
