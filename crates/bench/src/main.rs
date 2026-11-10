//! Megi benchmark runner.
//!
//! ```text
//! cargo run --release -p bench -- [--json] [--scale F] [--iterations N] ...
//! ```
//!
//! The binary has no external dependencies (beyond the engine's own workspace
//! crates) and uses `std::time::Instant` so it can run in any CI environment
//! without `criterion`, `hyperfine` or additional tooling.

#![forbid(unsafe_code)]

mod benches;
mod cli;
mod generators;
mod report;
mod runner;

use std::process::ExitCode;
use std::time::Instant;

use cli::{Options, Parsed};

fn main() -> ExitCode {
    let parsed = match Options::parse(std::env::args().skip(1)) {
        Ok(parsed) => parsed,
        Err(error) => {
            eprintln!("bench: {error}");
            eprintln!();
            eprint!("{}", cli::USAGE);
            return ExitCode::FAILURE;
        }
    };
    let options = match parsed {
        Parsed::Help => {
            print!("{}", cli::USAGE);
            return ExitCode::SUCCESS;
        }
        Parsed::List => {
            // Building the fixtures is part of registration; `--list` is a
            // convenience for humans, so paying the setup cost once is fine.
            let mut suite = runner::Suite::new();
            benches::register(&mut suite, &Options::default());
            report::print_list(&suite.names());
            return ExitCode::SUCCESS;
        }
        Parsed::Run(options) => options,
    };

    let setup_started = Instant::now();
    let mut suite = runner::Suite::new();
    benches::register(&mut suite, &options);
    let setup_ms = setup_started.elapsed().as_secs_f64() * 1000.0;

    let outcome = suite.run(&options, setup_ms);
    if options.json {
        report::print_json(&outcome);
    } else {
        report::print_table(&outcome);
    }
    ExitCode::SUCCESS
}
