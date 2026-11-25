use super::*;

#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};

/// Creates parent directories private to the user (0700 on Unix, #2307).
fn create_private_dirs(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true).mode(0o700);
        builder.create(path)?;
        // `mkdir` still applies the process umask, so restore the exact mode.
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
    }
    #[cfg(not(unix))]
    {
        fs::create_dir_all(path)
    }
}

/// Makes a newly created temporary file private to the user (0600 on Unix, #2307).
fn restrict_temporary(file: &fs::File) -> io::Result<()> {
    #[cfg(unix)]
    {
        file.set_permissions(fs::Permissions::from_mode(0o600))
    }
    #[cfg(not(unix))]
    {
        let _ = file;
        Ok(())
    }
}

/// Maximum size of a session file accepted from disk (bytes).
///
/// `fs::read_to_string` on an unbounded path (`/dev/zero`, a huge file or a FIFO) would
/// read until OOM; the cap also bounds the parsed JSON tree before `normalize` truncates.
pub const MAX_SESSION_BYTES: u64 = 4 * 1024 * 1024;

/// Result of loading a session file.
#[derive(Clone, Debug, PartialEq)]
pub struct LoadedSession {
    /// Loaded session when the file exists and is valid.
    pub session: Option<Session>,
    /// Human-readable warning for corrupt/unsupported files (also printed by the shell).
    pub warning: Option<String>,
}

/// Loads a session file. A missing file is a fresh session without a warning; any
/// corrupt/unsupported content returns `session = None` plus a warning (never panics).
pub fn load(path: &Path) -> LoadedSession {
    let mut bytes = Vec::new();
    let read = fs::File::open(path).and_then(|file| {
        let mut reader = file.take(MAX_SESSION_BYTES + 1);
        reader.read_to_end(&mut bytes)
    });
    match read {
        Ok(_) if bytes.len() as u64 > MAX_SESSION_BYTES => LoadedSession {
            session: None,
            warning: Some(format!(
                "ignoring session file {} larger than {} bytes",
                path.display(),
                MAX_SESSION_BYTES
            )),
        },
        Ok(_) => match String::from_utf8(bytes)
            .map_err(|error| error.utf8_error().to_string())
            .and_then(|text| Session::from_json(&text))
        {
            Ok(session) => LoadedSession {
                session: Some(session),
                warning: None,
            },
            Err(error) => LoadedSession {
                session: None,
                warning: Some(format!(
                    "ignoring corrupt session file {}: {error}",
                    path.display()
                )),
            },
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => LoadedSession {
            session: None,
            warning: None,
        },
        Err(error) => LoadedSession {
            session: None,
            warning: Some(format!(
                "cannot read session file {}: {error}",
                path.display()
            )),
        },
    }
}

/// Saves the session atomically (unique temporary + rename), creating parent directories.
pub fn save(path: &Path, session: &Session) -> io::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        create_private_dirs(parent)?;
    }
    let json = session.to_json();
    if json.len() as u64 > MAX_SESSION_BYTES {
        // `load` rejects files over the cap: writing one would make the next start discard
        // the session and overwrite the previous file.
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "session is {} bytes, larger than the {MAX_SESSION_BYTES}-byte read cap",
                json.len()
            ),
        ));
    }
    let temporary = write_temporary(path, json.as_bytes())?;
    let result = fs::rename(&temporary, path);
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// Removes a newly created temporary file unless the write succeeded and the guard was
/// disarmed (issue #1462).
pub(crate) struct TemporaryFileGuard {
    path: PathBuf,
    remove: bool,
}

impl TemporaryFileGuard {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self { path, remove: true }
    }

    pub(crate) fn keep(mut self) {
        self.remove = false;
    }
}

impl Drop for TemporaryFileGuard {
    fn drop(&mut self) {
        if self.remove {
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// Creates a fresh temporary next to `path` and writes `contents` into it.
///
/// `create_new` prevents following a symlink or truncating a pre-existing file planted at a
/// predictable name; an `AlreadyExists` collision advances the sequence and retries.
pub(crate) fn write_temporary(path: &Path, contents: &[u8]) -> io::Result<PathBuf> {
    for _ in 0..16 {
        let candidate = unique_temporary_path(path);
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        match options.open(&candidate) {
            Ok(mut file) => {
                // A short write (disk full/quota) must not leave the temporary file
                // behind; the guard removes it on every early return (#1462).
                let guard = TemporaryFileGuard::new(candidate.clone());
                // The umask applies to `create_new` too, so set the exact mode; the file
                // is already owner-only when created and never becomes more permissive.
                restrict_temporary(&file)?;
                if let Err(error) = file.write_all(contents) {
                    drop(file);
                    return Err(error); // dropping the guard removes the partial file
                }
                guard.keep();
                return Ok(candidate);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "cannot create a unique session temporary",
    ))
}

/// Returns a per-process, per-call temporary path next to `path` (same filesystem, so the
/// final rename is atomic).
///
/// A fixed `session.json.tmp` let two shell instances write and rename the same temporary,
/// producing a torn session file.
pub(crate) fn unique_temporary_path(path: &Path) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "session.json".to_string());
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    path.with_file_name(format!(".{name}.{}.{sequence}.tmp", std::process::id()))
}

/// Resolves the default session path from environment-like inputs (pure; tested).
///
/// Order: `$XDG_DATA_HOME/megi/session.json`, then `$HOME/.local/share/megi/session.json`.
/// Returns `None` when neither variable is usable.
pub fn default_session_path(xdg_data_home: Option<&Path>, home: Option<&Path>) -> Option<PathBuf> {
    if let Some(xdg) = xdg_data_home
        && !xdg.as_os_str().is_empty()
    {
        return Some(xdg.join("megi").join(SESSION_FILE_NAME));
    }
    if let Some(home) = home
        && !home.as_os_str().is_empty()
    {
        let standard = home
            .join(".local")
            .join("share")
            .join("megi")
            .join(SESSION_FILE_NAME);
        return Some(standard);
    }
    None
}

/// Runtime session path: explicit `--session-file` or the environment-based default.
pub fn resolve_session_path(explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = explicit {
        return Some(path.to_path_buf());
    }
    let xdg = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from);
    let home = std::env::var_os("HOME").map(PathBuf::from);
    default_session_path(xdg.as_deref(), home.as_deref())
}
