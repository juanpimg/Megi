//! Output formatting: a fixed-width human table and a stable JSON schema.
//!
//! The JSON schema (`megi-bench-v1`) is meant to be consumed by scripts and by
//! `docs/PERFORMANCE.md`; its field names are part of the contract, so new
//! fields are only ever *added*.

use std::fmt::Write as _;

use crate::runner::{BenchResult, RunOutcome, Throughput};

/// Prints the human-readable suite report.
pub fn print_table(outcome: &RunOutcome) {
    let meta = &outcome.meta;
    println!(
        "Megi benchmark suite — commit {}{}",
        meta.commit,
        if meta.dirty { " (worktree dirty)" } else { "" }
    );
    println!(
        "{} · {} · {} logical cores · scale {:.2} · {} iterations (+{} warm-up) · setup {:.1} ms",
        meta.rustc,
        meta.cpu,
        meta.cores,
        outcome.scale,
        outcome.iterations,
        outcome.warmup,
        outcome.setup_ms
    );
    if outcome.results.is_empty() {
        println!("\nNo benchmark matched the filter.");
        print_skipped(outcome);
        return;
    }
    let name_width = outcome
        .results
        .iter()
        .map(|result| result.name.len())
        .max()
        .unwrap_or(0)
        .max("benchmark".len());
    let detail_width = outcome
        .results
        .iter()
        .map(|result| result.detail.len())
        .max()
        .unwrap_or(0)
        .max("workload".len());
    println!();
    println!(
        "{:<name_width$}  {:<detail_width$}  {:>5}  {:>10}  {:>10}  {:>14}",
        "benchmark", "workload", "iters", "median ms", "min ms", "throughput"
    );
    println!("{}", "-".repeat(name_width + detail_width + 47));
    for result in &outcome.results {
        println!(
            "{:<name_width$}  {:<detail_width$}  {:>5}  {:>10.3}  {:>10.3}  {:>14}",
            result.name,
            result.detail,
            result.iterations,
            result.median_ms,
            result.min_ms,
            throughput_label(result)
        );
    }
    println!("{}", "-".repeat(name_width + detail_width + 47));
    println!(
        "{:<name_width$}  {:<detail_width$}  {:>5}  {:>10.3}",
        "TOTAL", "sum of medians", "", outcome.total_median_ms
    );
    print_skipped(outcome);
    println!();
    println!(
        "Notes: heavy benchmarks cap timed iterations at 3. Use --scale to trade \
         workload size for runtime; --filter to isolate a group."
    );
}

fn print_skipped(outcome: &RunOutcome) {
    if outcome.skipped.is_empty() {
        return;
    }
    println!();
    for (name, reason) in &outcome.skipped {
        println!("SKIP {name} — {reason}");
    }
}

fn throughput_label(result: &BenchResult) -> String {
    let Some(Throughput {
        unit, per_second, ..
    }) = &result.throughput
    else {
        return String::new();
    };
    let amount = if *per_second >= 1000.0 {
        format!("{per_second:.0}")
    } else if *per_second >= 10.0 {
        format!("{per_second:.1}")
    } else {
        format!("{per_second:.2}")
    };
    format!("{amount} {unit}/s")
}

/// Prints the machine-readable report (`--json`).
pub fn print_json(outcome: &RunOutcome) {
    let meta = &outcome.meta;
    let mut json = String::with_capacity(4096);
    json.push_str("{\n");
    json.push_str("  \"schema\": \"megi-bench-v1\",\n");
    let _ = writeln!(json, "  \"generated_unix_ms\": {},", meta.generated_unix_ms);
    json.push_str("  \"commit\": ");
    push_json_string(&mut json, &meta.commit);
    json.push_str(",\n");
    let _ = writeln!(json, "  \"worktree_dirty\": {},", meta.dirty);
    json.push_str("  \"rustc\": ");
    push_json_string(&mut json, &meta.rustc);
    json.push_str(",\n");
    json.push_str("  \"cpu\": ");
    push_json_string(&mut json, &meta.cpu);
    json.push_str(",\n");
    let _ = writeln!(json, "  \"cores\": {},", meta.cores);
    let _ = writeln!(json, "  \"scale\": {:.4},", outcome.scale);
    let _ = writeln!(json, "  \"iterations\": {},", outcome.iterations);
    let _ = writeln!(json, "  \"warmup\": {},", outcome.warmup);
    let _ = writeln!(json, "  \"setup_ms\": {:.3},", outcome.setup_ms);
    let _ = writeln!(
        json,
        "  \"total_median_ms\": {:.3},",
        outcome.total_median_ms
    );
    json.push_str("  \"results\": [\n");
    for (index, result) in outcome.results.iter().enumerate() {
        let comma = if index + 1 == outcome.results.len() {
            ""
        } else {
            ","
        };
        json.push_str("    {\n");
        json.push_str("      \"name\": ");
        push_json_string(&mut json, result.name);
        json.push_str(",\n");
        json.push_str("      \"group\": ");
        push_json_string(&mut json, result.group);
        json.push_str(",\n");
        json.push_str("      \"detail\": ");
        push_json_string(&mut json, &result.detail);
        json.push_str(",\n");
        let _ = writeln!(json, "      \"iterations\": {},", result.iterations);
        let _ = writeln!(json, "      \"median_ms\": {:.3},", result.median_ms);
        let _ = writeln!(json, "      \"min_ms\": {:.3},", result.min_ms);
        json.push_str("      \"samples_ms\": [");
        for (sample_index, sample) in result.samples_ms.iter().enumerate() {
            if sample_index > 0 {
                json.push_str(", ");
            }
            let _ = write!(json, "{sample:.3}");
        }
        json.push_str("],\n");
        match &result.throughput {
            Some(Throughput {
                count,
                unit,
                per_second,
            }) => {
                let _ = writeln!(
                    json,
                    "      \"throughput\": {{\"count\": {count:.3}, \"unit\": \"{unit}\", \
                     \"per_second\": {per_second:.3}}}"
                );
            }
            None => json.push_str("      \"throughput\": null\n"),
        }
        json.push_str("    }");
        json.push_str(comma);
        json.push('\n');
    }
    json.push_str("  ],\n");
    json.push_str("  \"skipped\": [\n");
    for (index, (name, reason)) in outcome.skipped.iter().enumerate() {
        let comma = if index + 1 == outcome.skipped.len() {
            ""
        } else {
            ","
        };
        json.push_str("    {\"name\": ");
        push_json_string(&mut json, name);
        json.push_str(", \"reason\": ");
        push_json_string(&mut json, reason);
        json.push('}');
        json.push_str(comma);
        json.push('\n');
    }
    json.push_str("  ]\n");
    json.push_str("}\n");
    print!("{json}");
}

/// Appends `value` as a JSON string literal (with escaping).
fn push_json_string(out: &mut String, value: &str) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", ch as u32);
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
}

/// Prints one benchmark name per line (`--list`).
pub fn print_list(names: &[&str]) {
    for name in names {
        println!("{name}");
    }
}
