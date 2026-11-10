//! Command-line interface for the Megi benchmark runner.
//!
//! Hand-rolled (no external crates) so the benchmark crate stays dependency-free
//! beyond the engine's own workspace crates.

/// Help text printed by `bench --help` and on argument errors.
pub const USAGE: &str = "\
usage: bench [options]

  --json              emit machine-readable JSON instead of the human table
  --scale <factor>    scale every synthetic workload (default 1.0; e.g. 0.25)
  --iterations <n>    timed runs per benchmark (default 5; heavy cases cap at 3)
  --warmup <n>        untimed warm-up runs per benchmark (default 1)
  --filter <text>     only run benchmarks whose `group/name` contains <text>
  --list              list the registered benchmark names and exit
  -h, --help          show this help

Examples:
  cargo run --release -p bench
  cargo run --release -p bench -- --json --scale 0.5
  ./tools/bench.sh --filter html
";

/// Parsed command line: either run the suite or handle an informational flag.
#[derive(Debug)]
pub enum Parsed {
    Run(Options),
    Help,
    List,
}

/// Benchmark suite options shared by the runner and the benchmark registry.
#[derive(Clone, Debug)]
pub struct Options {
    pub json: bool,
    pub scale: f64,
    pub iterations: usize,
    pub warmup: usize,
    pub filter: Option<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            json: false,
            scale: 1.0,
            iterations: 5,
            warmup: 1,
            filter: None,
        }
    }
}

impl Options {
    /// Parses `args` (without the program name).
    ///
    /// Returns an error message for unknown flags or malformed values; callers
    /// are expected to print [`USAGE`] alongside it.
    pub fn parse<I>(args: I) -> Result<Parsed, String>
    where
        I: IntoIterator<Item = String>,
    {
        let mut opts = Options::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--json" => opts.json = true,
                "--list" => return Ok(Parsed::List),
                "-h" | "--help" => return Ok(Parsed::Help),
                "--scale" => {
                    let raw = args
                        .next()
                        .ok_or_else(|| "--scale requires a value".to_string())?;
                    let value = raw
                        .parse::<f64>()
                        .map_err(|_| format!("invalid --scale value: {raw}"))?;
                    if !value.is_finite() || value <= 0.0 {
                        return Err(format!("--scale must be a positive number, got {raw}"));
                    }
                    opts.scale = value;
                }
                "--iterations" => {
                    let raw = args
                        .next()
                        .ok_or_else(|| "--iterations requires a value".to_string())?;
                    let value = raw
                        .parse::<usize>()
                        .map_err(|_| format!("invalid --iterations value: {raw}"))?;
                    if value == 0 {
                        return Err("--iterations must be at least 1".to_string());
                    }
                    opts.iterations = value;
                }
                "--warmup" => {
                    let raw = args
                        .next()
                        .ok_or_else(|| "--warmup requires a value".to_string())?;
                    opts.warmup = raw
                        .parse::<usize>()
                        .map_err(|_| format!("invalid --warmup value: {raw}"))?;
                }
                "--filter" => {
                    let raw = args
                        .next()
                        .ok_or_else(|| "--filter requires a value".to_string())?;
                    opts.filter = Some(raw);
                }
                other => return Err(format!("unknown argument: {other}")),
            }
        }
        Ok(Parsed::Run(opts))
    }
}

#[cfg(test)]
mod tests {
    use super::{Options, Parsed};

    fn parse(args: &[&str]) -> Result<Parsed, String> {
        Options::parse(args.iter().map(|arg| (*arg).to_string()))
    }

    #[test]
    fn defaults_are_sane() {
        let Ok(Parsed::Run(options)) = parse(&[]) else {
            panic!("default invocation must run the suite");
        };
        assert_eq!(options.iterations, 5);
        assert_eq!(options.warmup, 1);
        assert!(!options.json);
        assert!((options.scale - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn flags_are_parsed() {
        let Ok(Parsed::Run(options)) = parse(&[
            "--json",
            "--scale",
            "0.25",
            "--iterations",
            "2",
            "--filter",
            "html",
        ]) else {
            panic!("valid flags must parse");
        };
        assert!(options.json);
        assert!((options.scale - 0.25).abs() < f64::EPSILON);
        assert_eq!(options.iterations, 2);
        assert_eq!(options.filter.as_deref(), Some("html"));
    }

    #[test]
    fn malformed_arguments_are_rejected() {
        assert!(parse(&["--unknown"]).is_err());
        assert!(parse(&["--scale"]).is_err());
        assert!(parse(&["--scale", "nope"]).is_err());
        assert!(parse(&["--scale", "0"]).is_err());
        assert!(parse(&["--iterations", "0"]).is_err());
    }
}
