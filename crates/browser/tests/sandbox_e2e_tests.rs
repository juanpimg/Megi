//! M3#4: real Linux renderer sandbox, 12 negative probes + 1 render smoke.
//!
//! Each negative test re-execs this test binary with `sandbox_probe` selected by an exact
//! filter. That child applies the same `megi_sandbox::apply_renderer_sandbox` API as the
//! renderer worker, then attempts a forbidden operation. The parent asserts the operation
//! was denied with `EACCES` and that the mode reported by the child is `enforced`.

use megi_browser::ipc::{
    IpcRenderRequest, ProcessRenderer, request_sandbox_allows_network, request_sandbox_read_path,
};
use megi_sandbox::render_security_matrix;
use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::net::{TcpListener, TcpStream};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

const PROBE: &str = "MEGI_SANDBOX_PROBE";
const SCRATCH: &str = "MEGI_SANDBOX_SCRATCH";
const OUTSIDE: &str = "MEGI_SANDBOX_OUTSIDE";
const ALLOW_NETWORK: &str = "MEGI_SANDBOX_ALLOW_NETWORK";
const PROBE_PORT: &str = "MEGI_SANDBOX_PORT";
static COUNTER: AtomicU64 = AtomicU64::new(0);

struct ProbeResult {
    mode: String,
    denied: bool,
    detail: String,
}

fn private_dir(tag: &str) -> PathBuf {
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("megi-sandbox-{tag}-{}-{id}", std::process::id()));
    std::fs::create_dir_all(&path).expect("create private probe directory");
    path
}

fn write_result(scratch: &Path, mode: &str, denied: bool, detail: &str) {
    let safe_detail = detail.replace(['|', '\n'], "/");
    let text = format!("{mode}|{denied}|{safe_detail}");
    std::fs::write(scratch.join("result.txt"), text).expect("write probe result");
}

fn run_probe(op: &str, outside: &Path) -> io::Result<()> {
    match op {
        "etc_passwd" => File::open("/etc/passwd").map(|_| ()),
        "etc_dir" => std::fs::read_dir("/etc").map(|_| ()),
        "outside_write" => File::create(outside.join("write.txt")).map(|_| ()),
        "outside_truncate" => OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(outside.join("truncate.txt"))
            .map(|_| ()),
        "outside_remove" => std::fs::remove_file(outside.join("remove.txt")),
        "outside_rename" => {
            std::fs::rename(outside.join("rename.txt"), outside.join("renamed.txt"))
        }
        "outside_mkdir" => std::fs::create_dir(outside.join("newdir")),
        "outside_mksock" => UnixListener::bind(outside.join("probe.sock")).map(|_| ()),
        "outside_unix_connect" => UnixStream::connect(outside.join("listener.sock")).map(|_| ()),
        "system_exec" => Command::new("/usr/bin/true").status().map(|_| ()),
        "tcp_connect" => {
            let port: u16 = std::env::var(PROBE_PORT)
                .expect("probe port")
                .parse()
                .expect("valid port");
            TcpStream::connect(("127.0.0.1", port)).map(|_| ())
        }
        "tcp_bind" => TcpListener::bind("127.0.0.1:0").map(|_| ()),
        other => Err(io::Error::other(format!("unknown probe: {other}"))),
    }
}

/// Child side of every negative test. Inert unless the parent selects it by env + filter.
#[test]
fn sandbox_probe() {
    let Ok(op) = std::env::var(PROBE) else {
        return;
    };
    let scratch = PathBuf::from(std::env::var(SCRATCH).expect("scratch path"));
    let outside = PathBuf::from(std::env::var(OUTSIDE).expect("outside path"));
    let allow_network = std::env::var(ALLOW_NETWORK)
        .map(|value| value != "0")
        .unwrap_or(false);
    let mode = match megi_sandbox::apply_renderer_sandbox(&scratch, allow_network) {
        Ok(mode) => mode,
        Err(error) => {
            write_result(&scratch, "error", false, &error.to_string());
            return;
        }
    };
    let attempt = run_probe(&op, &outside);
    let denied = matches!(&attempt, Err(error) if error.kind() == io::ErrorKind::PermissionDenied);
    let detail = match attempt {
        Ok(()) => "operation succeeded".to_owned(),
        Err(error) => error.to_string(),
    };
    write_result(&scratch, mode.as_str(), denied, &detail);
}

fn spawn_probe(op: &str) -> ProbeResult {
    let scratch = private_dir("scratch");
    let outside = private_dir("outside");
    for name in ["truncate.txt", "remove.txt", "rename.txt"] {
        std::fs::write(outside.join(name), b"seed").expect("seed outside file");
    }
    let tcp_listener = (op == "tcp_connect")
        .then(|| TcpListener::bind("127.0.0.1:0").expect("bind probe listener"));
    let unix_listener = (op == "outside_unix_connect")
        .then(|| UnixListener::bind(outside.join("listener.sock")).expect("bind unix listener"));

    let executable = std::env::current_exe().expect("current test binary");
    let mut command = Command::new(executable);
    command
        .args(["--exact", "sandbox_probe", "--nocapture"])
        .env(PROBE, op)
        .env(SCRATCH, &scratch)
        .env(OUTSIDE, &outside)
        .env(ALLOW_NETWORK, "0")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(listener) = &tcp_listener {
        command.env(
            PROBE_PORT,
            listener
                .local_addr()
                .expect("local addr")
                .port()
                .to_string(),
        );
    }
    let output = command.output().expect("spawn probe child");
    assert!(
        output.status.success(),
        "probe child failed: {:?}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let mut text = String::new();
    File::open(scratch.join("result.txt"))
        .expect("probe result file")
        .read_to_string(&mut text)
        .expect("read probe result");
    let mut parts = text.splitn(3, '|');
    let mode = parts.next().unwrap_or_default().to_owned();
    let denied = parts.next().unwrap_or_default() == "true";
    let detail = parts.next().unwrap_or_default().to_owned();
    drop(tcp_listener);
    drop(unix_listener);
    ProbeResult {
        mode,
        denied,
        detail,
    }
}

#[cfg(target_os = "linux")]
fn assert_denied(op: &str) {
    let result = spawn_probe(op);
    assert_eq!(
        result.mode, "enforced",
        "{op}: expected real enforcement, got {} ({})",
        result.mode, result.detail
    );
    assert!(
        result.denied,
        "{op}: operation was not denied: {}",
        result.detail
    );
}

#[cfg(target_os = "linux")]
#[test]
fn negative_01_read_etc_passwd_denied() {
    assert_denied("etc_passwd");
}

#[cfg(target_os = "linux")]
#[test]
fn negative_02_list_etc_directory_denied() {
    assert_denied("etc_dir");
}

#[cfg(target_os = "linux")]
#[test]
fn negative_03_write_outside_scratch_denied() {
    assert_denied("outside_write");
}

#[cfg(target_os = "linux")]
#[test]
fn negative_04_truncate_outside_scratch_denied() {
    assert_denied("outside_truncate");
}

#[cfg(target_os = "linux")]
#[test]
fn negative_05_remove_outside_scratch_denied() {
    assert_denied("outside_remove");
}

#[cfg(target_os = "linux")]
#[test]
fn negative_06_rename_outside_scratch_denied() {
    assert_denied("outside_rename");
}

#[cfg(target_os = "linux")]
#[test]
fn negative_07_mkdir_outside_scratch_denied() {
    assert_denied("outside_mkdir");
}

#[cfg(target_os = "linux")]
#[test]
fn negative_08_make_unix_socket_outside_denied() {
    assert_denied("outside_mksock");
}

#[cfg(target_os = "linux")]
#[test]
fn negative_09_connect_unix_socket_outside_denied() {
    assert_denied("outside_unix_connect");
}

#[cfg(target_os = "linux")]
#[test]
fn negative_10_execute_system_binary_denied() {
    assert_denied("system_exec");
}

#[cfg(target_os = "linux")]
#[test]
fn negative_11_tcp_connect_denied() {
    assert_denied("tcp_connect");
}

#[cfg(target_os = "linux")]
#[test]
fn negative_12_tcp_bind_denied() {
    assert_denied("tcp_bind");
}

/// Smoke: a real `--megi-ipc-worker` still paints and publishes its enforcement level.
#[cfg(target_os = "linux")]
#[test]
fn smoke_real_renderer_reports_enforced_mode() {
    assert_eq!(
        render_security_matrix(),
        "linux: enforced; macos: unsupported; windows: unsupported"
    );
    let executable = std::path::Path::new(env!("CARGO_BIN_EXE_megi"));
    let mut renderer = ProcessRenderer::spawn_with_executable(executable).expect("spawn renderer");
    let request = IpcRenderRequest {
        generation: 1,
        input: "data:text/html,<title>Sandbox</title><p>enforced</p>".to_owned(),
        css_width: 320,
        css_height: 200,
        base_override: None,
        scripting: false,
        find_query: None,
        options: Default::default(),
    };
    assert!(!request_sandbox_allows_network(&request));
    let mut remote = request.clone();
    remote.input = "https://example.test/".to_owned();
    assert!(request_sandbox_allows_network(&remote));
    assert!(request_sandbox_read_path(&request).is_none());
    let mut file_request = request.clone();
    file_request.input = format!(
        "file://{}",
        std::env::current_exe().expect("current exe").display()
    );
    assert_eq!(
        request_sandbox_read_path(&file_request),
        Some(std::env::current_exe().expect("current exe"))
    );
    let response = renderer.render(request).expect("render in sandbox");
    assert!(
        response.error.is_none(),
        "render error: {:?}",
        response.error
    );
    assert!(
        !response.png_bytes.is_empty(),
        "sandboxed renderer must paint"
    );
    assert_eq!(response.sandbox_mode, "enforced");
}
