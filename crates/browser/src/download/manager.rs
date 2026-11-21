//! Bounded download manager with pause/resume/cancel and deterministic progress (F7-SAVE-AS / #988).
//!
//! Pure state machine: no I/O, no wall clock. The shell feeds byte counts into it and uses
//! its state to decide whether a resource fetch may continue, be paused, resumed or canceled.

use super::DownloadKind;

/// Lifecycle of one download job.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadState {
    Queued,
    Downloading,
    Paused,
    Completed,
    Canceled,
    Failed,
}

/// One tracked download job.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadJob {
    pub id: u64,
    pub url: String,
    pub kind: DownloadKind,
    pub received: u64,
    pub total: Option<u64>,
    pub state: DownloadState,
    pub error: Option<String>,
}

impl DownloadJob {
    /// Short status name for UI display.
    pub fn state_name(&self) -> &'static str {
        match self.state {
            DownloadState::Queued => "queued",
            DownloadState::Downloading => "downloading",
            DownloadState::Paused => "paused",
            DownloadState::Completed => "completed",
            DownloadState::Canceled => "canceled",
            DownloadState::Failed => "failed",
        }
    }
}

/// Default caps for the shell's download manager.
pub const DEFAULT_MAX_JOBS: usize = 64;
pub const DEFAULT_MAX_ACTIVE: usize = 4;
pub const DEFAULT_MAX_BYTES: u64 = 256 * 1024 * 1024;

/// Bounded, deterministic download manager.
#[derive(Clone, Debug)]
pub struct DownloadManager {
    jobs: Vec<DownloadJob>,
    next_id: u64,
    max_jobs: usize,
    max_active: usize,
    max_bytes: u64,
}

impl DownloadManager {
    pub fn new(max_jobs: usize, max_active: usize, max_bytes: u64) -> Self {
        Self {
            jobs: Vec::new(),
            next_id: 1,
            max_jobs,
            max_active: max_active.max(1),
            max_bytes,
        }
    }

    /// Queues a job; rejects when the job cap is reached.
    pub fn enqueue(
        &mut self,
        url: impl Into<String>,
        kind: DownloadKind,
        total: Option<u64>,
    ) -> Result<u64, String> {
        if self.jobs.len() >= self.max_jobs {
            return Err("download job limit exceeded".to_owned());
        }
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        self.jobs.push(DownloadJob {
            id,
            url: url.into(),
            kind,
            received: 0,
            total,
            state: DownloadState::Queued,
            error: None,
        });
        Ok(id)
    }

    /// Starts a queued job, respecting the concurrent-active cap.
    pub fn start(&mut self, id: u64) -> Result<(), String> {
        let (active, max_active) = (self.active_count(), self.max_active);
        let job = self.job_mut(id)?;
        if job.state != DownloadState::Queued {
            return Err(format!("job {id} is not queued"));
        }
        if active >= max_active {
            return Err("too many active downloads".to_owned());
        }
        job.state = DownloadState::Downloading;
        Ok(())
    }

    /// Records received bytes; rejects progress on non-downloading jobs or past the byte cap.
    pub fn progress(&mut self, id: u64, bytes: u64) -> Result<(), String> {
        let max_bytes = self.max_bytes;
        let job = self.job_mut(id)?;
        if job.state != DownloadState::Downloading {
            return Err(format!("job {id} is not downloading"));
        }
        let total_received = job.received.saturating_add(bytes);
        if total_received > max_bytes {
            return Err("download byte cap exceeded".to_owned());
        }
        if job.total.is_some_and(|expected| total_received > expected) {
            return Err("download exceeded declared size".to_owned());
        }
        job.received = total_received;
        Ok(())
    }

    pub fn pause(&mut self, id: u64) -> Result<(), String> {
        let job = self.job_mut(id)?;
        if job.state != DownloadState::Downloading {
            return Err(format!("job {id} is not downloading"));
        }
        job.state = DownloadState::Paused;
        Ok(())
    }

    pub fn resume(&mut self, id: u64) -> Result<(), String> {
        let (active, max_active) = (self.active_count(), self.max_active);
        let job = self.job_mut(id)?;
        if job.state != DownloadState::Paused {
            return Err(format!("job {id} is not paused"));
        }
        if active >= max_active {
            return Err("too many active downloads".to_owned());
        }
        job.state = DownloadState::Downloading;
        Ok(())
    }

    pub fn cancel(&mut self, id: u64) -> Result<(), String> {
        let job = self.job_mut(id)?;
        if matches!(
            job.state,
            DownloadState::Completed | DownloadState::Canceled
        ) {
            return Err(format!("job {id} already finished"));
        }
        job.state = DownloadState::Canceled;
        Ok(())
    }

    pub fn complete(&mut self, id: u64) -> Result<(), String> {
        let job = self.job_mut(id)?;
        if job.state != DownloadState::Downloading {
            return Err(format!("job {id} is not downloading"));
        }
        job.state = DownloadState::Completed;
        Ok(())
    }

    pub fn fail(&mut self, id: u64, reason: impl Into<String>) -> Result<(), String> {
        let job = self.job_mut(id)?;
        job.state = DownloadState::Failed;
        job.error = Some(reason.into());
        Ok(())
    }

    pub fn get(&self, id: u64) -> Option<&DownloadJob> {
        self.jobs.iter().find(|job| job.id == id)
    }

    pub fn jobs(&self) -> &[DownloadJob] {
        &self.jobs
    }

    pub fn active_count(&self) -> usize {
        self.jobs
            .iter()
            .filter(|job| job.state == DownloadState::Downloading)
            .count()
    }

    /// Deterministic progress text in the status bar (percent from received/total).
    pub fn progress_text(&self, id: u64) -> Option<String> {
        let job = self.get(id)?;
        let prefix = match job.state {
            DownloadState::Downloading => "descargando",
            DownloadState::Paused => "pausada",
            DownloadState::Completed => "completada",
            DownloadState::Canceled => "cancelada",
            DownloadState::Failed => "fallida",
            DownloadState::Queued => "en cola",
        };
        match job.total {
            Some(total) if total > 0 => {
                let percent = (job.received.saturating_mul(100) / total).min(100);
                Some(format!("{prefix} {}% {}", percent, job.url))
            }
            _ => Some(format!("{prefix} {}", job.url)),
        }
    }

    fn job_mut(&mut self, id: u64) -> Result<&mut DownloadJob, String> {
        self.jobs
            .iter_mut()
            .find(|job| job.id == id)
            .ok_or_else(|| format!("unknown download job {id}"))
    }
}

#[cfg(test)]
mod manager_tests {
    use super::*;

    fn manager() -> DownloadManager {
        DownloadManager::new(4, 2, 1024)
    }

    #[test]
    fn pause_resume_and_cancel_are_typed_transitions() {
        let mut manager = manager();
        let id = manager
            .enqueue("https://example.test/a.js", DownloadKind::Script, Some(100))
            .unwrap();
        assert_eq!(manager.get(id).unwrap().state, DownloadState::Queued);
        assert!(manager.cancel(id).is_ok(), "queued jobs can be canceled");
        assert!(manager.start(id).is_err(), "canceled jobs cannot start");

        let id2 = manager
            .enqueue("https://example.test/b.css", DownloadKind::Css, Some(100))
            .unwrap();
        manager.start(id2).unwrap();
        manager.progress(id2, 40).unwrap();
        manager.pause(id2).unwrap();
        assert!(
            manager.progress(id2, 10).is_err(),
            "paused jobs reject progress"
        );
        manager.resume(id2).unwrap();
        manager.progress(id2, 60).unwrap();
        manager.complete(id2).unwrap();
        assert_eq!(manager.get(id2).unwrap().received, 100);
        assert_eq!(
            manager.progress_text(id2).as_deref(),
            Some("completada 100% https://example.test/b.css")
        );
        assert!(manager.resume(id2).is_err(), "completed jobs cannot resume");
    }

    #[test]
    fn caps_are_enforced() {
        let mut manager = manager();
        let a = manager.enqueue("a", DownloadKind::Image, Some(10)).unwrap();
        let b = manager.enqueue("b", DownloadKind::Image, Some(10)).unwrap();
        let c = manager.enqueue("c", DownloadKind::Image, Some(10)).unwrap();
        manager.start(a).unwrap();
        manager.start(b).unwrap();
        assert!(manager.start(c).is_err(), "active cap");
        assert_eq!(manager.active_count(), 2);
        manager.pause(a).unwrap();
        manager.start(c).unwrap();
        assert!(manager.progress(c, 2000).is_err(), "byte cap");
        assert!(manager.progress(b, 11).is_err(), "declared size cap");
    }
}
