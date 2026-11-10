//! Benchmark registry, timing harness and environment metadata.
//!
//! The runner owns three concerns:
//!
//! 1. Registering named benchmarks ([`BenchEntry`]) that own their pre-built
//!    fixtures (fixture construction is deliberately *not* timed).
//! 2. Running each registered closure `warmup + iterations` times and
//!    summarising the samples with median and minimum.
//! 3. Capturing the environment (commit, worktree dirtiness, CPU, timestamp)
//!    so a baseline document can state exactly what was measured.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::cli::Options;

/// Boxed benchmark body; captures its own fixtures and may mutate them.
pub type BenchFn = Box<dyn FnMut()>;

/// One registered benchmark.
pub struct BenchEntry {
    /// Stable identifier, e.g. `html/nested-div-64k`.
    pub name: &'static str,
    /// Coarse group used by `--filter`, e.g. `html`.
    pub group: &'static str,
    /// Human-readable workload description (sizes, counts, viewport).
    pub detail: String,
    /// Work performed per run, used to derive throughput.
    pub work: Option<(f64, &'static str)>,
    /// Heavy benchmarks get fewer timed iterations to keep the suite short.
    pub heavy: bool,
    /// The measured body.
    pub run: BenchFn,
}

impl BenchEntry {
    /// Creates an entry from a closure. The closure must own its fixtures.
    pub fn new<F>(
        name: &'static str,
        group: &'static str,
        detail: impl Into<String>,
        run: F,
    ) -> Self
    where
        F: FnMut() + 'static,
    {
        Self {
            name,
            group,
            detail: detail.into(),
            work: None,
            heavy: false,
            run: Box::new(run),
        }
    }

    /// Declares the amount of work per run so the report can show throughput.
    pub fn with_units(mut self, count: f64, unit: &'static str) -> Self {
        self.work = Some((count, unit));
        self
    }

    /// Marks the benchmark as expensive (caps timed iterations at three).
    pub fn heavy(mut self) -> Self {
        self.heavy = true;
        self
    }
}

/// The ordered benchmark suite under construction.
#[derive(Default)]
pub struct Suite {
    entries: Vec<BenchEntry>,
    skipped: Vec<(String, String)>,
}

impl Suite {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, entry: BenchEntry) {
        self.entries.push(entry);
    }

    /// Records a benchmark that cannot run in this environment (fixture absent,
    /// optional feature disabled, ...). Skipped entries are reported, not hidden.
    pub fn skip(&mut self, name: impl Into<String>, reason: impl Into<String>) {
        self.skipped.push((name.into(), reason.into()));
    }

    /// Names of every registered benchmark (used by `--list`).
    pub fn names(&self) -> Vec<&'static str> {
        self.entries.iter().map(|entry| entry.name).collect()
    }

    /// Runs every benchmark that matches the filter.
    pub fn run(mut self, opts: &Options, setup_ms: f64) -> RunOutcome {
        let mut results = Vec::new();
        for entry in &mut self.entries {
            if let Some(filter) = &opts.filter
                && !entry.name.contains(filter.as_str())
                && !entry.group.contains(filter.as_str())
            {
                continue;
            }
            let iterations = if entry.heavy {
                opts.iterations.min(3)
            } else {
                opts.iterations
            };
            let warmup = if entry.heavy {
                opts.warmup.min(1)
            } else {
                opts.warmup
            };
            let (samples_ms, median_ms, min_ms) = measure(&mut entry.run, warmup, iterations);
            let throughput = entry.work.map(|(count, unit)| Throughput {
                count,
                unit,
                per_second: if median_ms > 0.0 {
                    count * 1000.0 / median_ms
                } else {
                    f64::INFINITY
                },
            });
            results.push(BenchResult {
                name: entry.name,
                group: entry.group,
                detail: entry.detail.clone(),
                iterations,
                median_ms,
                min_ms,
                samples_ms,
                throughput,
            });
        }
        let total_median_ms = results.iter().map(|result| result.median_ms).sum();
        RunOutcome {
            results,
            skipped: std::mem::take(&mut self.skipped),
            meta: Meta::capture(),
            setup_ms,
            total_median_ms,
            scale: opts.scale,
            iterations: opts.iterations,
            warmup: opts.warmup,
        }
    }
}

/// One benchmark result.
pub struct BenchResult {
    pub name: &'static str,
    pub group: &'static str,
    pub detail: String,
    pub iterations: usize,
    pub median_ms: f64,
    pub min_ms: f64,
    pub samples_ms: Vec<f64>,
    pub throughput: Option<Throughput>,
}

/// Derived throughput for benchmarks that declare their work amount.
pub struct Throughput {
    pub count: f64,
    pub unit: &'static str,
    pub per_second: f64,
}

/// Environment captured when the suite finished.
pub struct Meta {
    pub commit: String,
    pub dirty: bool,
    pub rustc: String,
    pub cpu: String,
    pub cores: usize,
    pub generated_unix_ms: u128,
}

impl Meta {
    fn capture() -> Self {
        let (commit, dirty) = git_state();
        Self {
            commit,
            dirty,
            rustc: rustc_version(),
            cpu: cpu_model(),
            cores: std::thread::available_parallelism()
                .map(|cores| cores.get())
                .unwrap_or(0),
            generated_unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|elapsed| elapsed.as_millis())
                .unwrap_or(0),
        }
    }
}

/// Everything produced by one suite run.
pub struct RunOutcome {
    pub results: Vec<BenchResult>,
    pub skipped: Vec<(String, String)>,
    pub meta: Meta,
    pub setup_ms: f64,
    pub total_median_ms: f64,
    pub scale: f64,
    pub iterations: usize,
    pub warmup: usize,
}

/// Runs `run` `warmup` times untimed, then `iterations` times timed.
///
/// Returns `(samples_ms, median_ms, min_ms)`.
fn measure(run: &mut BenchFn, warmup: usize, iterations: usize) -> (Vec<f64>, f64, f64) {
    for _ in 0..warmup {
        run();
    }
    let mut samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let started = Instant::now();
        run();
        samples.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    let median = median(&samples);
    let min = samples.iter().copied().fold(f64::INFINITY, f64::min);
    (samples, median, min)
}

/// Median of `samples` (mean of the two central values for even counts).
fn median(samples: &[f64]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let middle = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) {
        (sorted[middle - 1] + sorted[middle]) / 2.0
    } else {
        sorted[middle]
    }
}

/// Absolute path of the workspace root (`.../crates/bench` -> `...`).
pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/bench must live two levels below the workspace root")
        .to_path_buf()
}

/// Best-effort `(short commit, worktree dirty)` from Git.
fn git_state() -> (String, bool) {
    let root = workspace_root();
    let commit = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|commit| !commit.is_empty())
        .unwrap_or_else(|| "unknown".to_owned());
    let dirty = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .map(|output| !output.stdout.is_empty())
        .unwrap_or(false);
    (commit, dirty)
}

/// Version banner of the compiler that built this binary.
fn rustc_version() -> String {
    Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|version| !version.is_empty())
        .unwrap_or_else(|| String::from("rustc unknown"))
}

/// Human-readable CPU model, read from `/proc/cpuinfo` when available.
fn cpu_model() -> String {
    let Ok(contents) = std::fs::read_to_string("/proc/cpuinfo") else {
        return String::from("unknown");
    };
    for line in contents.lines() {
        if let Some((key, value)) = line.split_once(':')
            && key.trim().eq_ignore_ascii_case("model name")
        {
            return value.trim().to_owned();
        }
    }
    String::from("unknown")
}

#[cfg(test)]
mod tests {
    use super::median;

    #[test]
    fn median_handles_odd_and_even_samples() {
        assert_eq!(median(&[]), 0.0);
        assert_eq!(median(&[4.0]), 4.0);
        assert_eq!(median(&[3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(&[4.0, 1.0, 3.0, 2.0]), 2.5);
    }
}
