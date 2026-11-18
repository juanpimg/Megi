use super::*;

/// Best-effort advisory lock next to the session file (`session.json.lock`).
///
/// This is intentionally simple: `create_new` of the lock file; if it already exists the
/// caller proceeds without a lock (saves are atomic, so the worst case is last-writer-wins).
/// Stale lock files are not removed automatically (documented limitation).
#[derive(Debug)]
pub struct SessionLock {
    path: PathBuf,
}

impl SessionLock {
    /// Tries to acquire the lock; returns `None` when another process holds it.
    pub fn acquire(session_path: &Path) -> io::Result<Option<Self>> {
        let path = session_path.with_extension("json.lock");
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)?;
        }
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                let _ = writeln!(file, "{}", std::process::id());
                Ok(Some(Self { path }))
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Lock file path.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for SessionLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
