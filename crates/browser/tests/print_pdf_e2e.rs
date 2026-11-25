//! E2E evidence for M5#4 (headless printing): the real `megi` binary must
//! render a page and write a deterministic PDF 1.4 without a window or a
//! native print dialog. ISO 32000-1 §7.5: `startxref` locates the `xref`
//! table; `%%EOF` terminates the file.
//!
//! Native `window.print()` / print-dialog behaviour belongs to M11#3; this
//! test only covers the headless `--print-to-pdf` path.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// `/var/tmp` is explicit: the CI harness pins `TMPDIR=/var/tmp` and the
/// evidence paths must not depend on a possibly shorter-lived `/tmp`.
fn pdf_path(stem: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    PathBuf::from("/var/tmp").join(format!(
        "megi-pdf-e2e-{}-{nanos}-{stem}.pdf",
        std::process::id()
    ))
}

fn print_once(path: &Path) -> (String, Vec<u8>) {
    let output = Command::new(env!("CARGO_BIN_EXE_megi"))
        .arg("data:text/html,<h1>Megi PDF</h1>")
        .args(["--width", "800", "--height", "600", "--print-to-pdf"])
        .arg(path)
        .output()
        .expect("megi binary runs");
    assert!(
        output.status.success(),
        "megi --print-to-pdf failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    let bytes = std::fs::read(path).expect("PDF file exists");
    let reported = format!("wrote {} (800x600, {} bytes)", path.display(), bytes.len());
    assert!(
        stdout.lines().any(|line| line == reported),
        "stdout must report the real size {reported:?}, got {stdout:?}"
    );
    (stdout, bytes)
}

fn assert_pdf_structure(bytes: &[u8]) {
    assert!(bytes.starts_with(b"%PDF-1.4\n"), "PDF 1.4 header");
    assert!(
        find(bytes, b"/MediaBox [0 0 800 600]").is_some(),
        "the page is 800x600 points"
    );
    let xref = find(bytes, b"xref\n0 ").expect("xref table");
    let startxref = find(bytes, b"startxref\n").expect("startxref");
    let value_start = startxref + b"startxref\n".len();
    let line_end =
        value_start + find(&bytes[value_start..], b"\n").expect("newline after the offset");
    let declared: usize = std::str::from_utf8(&bytes[value_start..line_end])
        .expect("ascii offset")
        .parse()
        .expect("numeric offset");
    assert!(
        declared < bytes.len() && bytes[declared..].starts_with(b"xref\n0 "),
        "startxref offset {declared} must point at the xref table at {xref}"
    );
    assert!(bytes.ends_with(b"%%EOF\n"), "the file ends with %%EOF");
}

#[test]
fn headless_print_to_pdf_writes_a_deterministic_pdf_1_4() {
    let first = pdf_path("a");
    let second = pdf_path("b");
    let (first_stdout, first_bytes) = print_once(&first);
    let (second_stdout, second_bytes) = print_once(&second);
    assert_pdf_structure(&first_bytes);
    assert_pdf_structure(&second_bytes);
    assert_eq!(
        first_bytes, second_bytes,
        "two headless runs of the same URL must produce byte-identical PDFs"
    );
    let reported_size = |stdout: &str| {
        stdout
            .lines()
            .find(|line| line.starts_with("wrote "))
            .and_then(|line| line.rsplit_once(" ("))
            .map(|(_, suffix)| suffix.to_string())
    };
    assert_eq!(
        reported_size(&first_stdout),
        reported_size(&second_stdout),
        "the reported byte count must be identical across runs"
    );
    let _ = std::fs::remove_file(&first);
    let _ = std::fs::remove_file(&second);
}
