//! M7 profiler: exports the wall-clock breakdown measured by the render pipeline.
//!
//! `RenderTimings` is the only source of values here. The DevTools page carries a copy of
//! the timings captured for the attached render, so a profile cannot drift from the page
//! it describes. Durations are microseconds (CDP-style) and the trace uses complete
//! events (`ph = "X"`), which a flamegraph viewer can stack directly.

use std::time::Duration;

use megi_webview::render::RenderTimings;
use serde_json::{Value, json};

/// One exported phase: stable label plus the measured duration.
type Phase = (&'static str, Duration);

/// Returns the real phases of `timings`, including the pipeline total.
///
/// Keeping the label/Duration pairs in one table makes metrics and trace exports agree.
fn phases(timings: &RenderTimings) -> Vec<Phase> {
    vec![
        ("Fetch", timings.fetch),
        ("DecodeParse", timings.decode_parse),
        ("Styles", timings.styles),
        ("StylesPreScript", timings.styles_pre_script),
        ("Fonts", timings.fonts),
        ("Layout", timings.layout),
        ("Images", timings.images),
        ("Backgrounds", timings.backgrounds),
        ("Scripting", timings.scripting),
        ("EventLoop", timings.event_loop),
        ("Paint", timings.paint),
        ("Raster", timings.raster),
        ("Total", timings.total),
    ]
}

/// Converts a measured duration into the JSON-safe microsecond unit used by both exports.
fn micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

/// `Performance.getMetrics`: phase breakdown in microseconds from the attached render.
pub(super) fn get_metrics(timings: &RenderTimings, id: Value) -> String {
    let metrics: Vec<Value> = phases(timings)
        .into_iter()
        .map(|(name, duration)| json!({ "name": name, "value": micros(duration) }))
        .collect();
    json!({ "id": id, "result": { "metrics": metrics } }).to_string()
}

/// `Performance.getTrace`: complete-event phase trace, ready to render as a flamegraph.
pub(super) fn get_trace(timings: &RenderTimings, id: Value) -> String {
    let trace_events: Vec<Value> = phases(timings)
        .into_iter()
        .map(|(name, duration)| {
            json!({
                "name": name,
                "cat": "devtools.timeline",
                "ph": "X",
                "ts": 0,
                "pid": 1,
                "tid": 1,
                "args": { "dur": micros(duration) },
            })
        })
        .collect();
    json!({
        "id": id,
        "result": {
            "traceEvents": trace_events,
            "source": "megi.RenderTimings",
        },
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use megi_webview::render::RenderTimings;
    use serde_json::json;

    use super::{get_metrics, get_trace, micros};

    #[test]
    fn micros_uses_json_safe_integers() {
        assert_eq!(micros(Duration::from_micros(17)), 17);
        assert_eq!(micros(Duration::MAX), u64::MAX);
    }

    #[test]
    fn metrics_and_trace_report_the_same_measured_phase() {
        let timings = RenderTimings {
            layout: Duration::from_micros(123),
            total: Duration::from_micros(456),
            ..Default::default()
        };
        let metrics: serde_json::Value =
            serde_json::from_str(&get_metrics(&timings, json!(1))).expect("metrics JSON");
        let trace: serde_json::Value =
            serde_json::from_str(&get_trace(&timings, json!(2))).expect("trace JSON");
        assert_eq!(metrics["result"]["metrics"][5]["name"], "Layout");
        assert_eq!(metrics["result"]["metrics"][5]["value"], 123);
        assert_eq!(trace["result"]["traceEvents"][5]["name"], "Layout");
        assert_eq!(trace["result"]["traceEvents"][5]["args"]["dur"], 123);
        assert_eq!(metrics["result"]["metrics"][12]["name"], "Total");
        assert_eq!(metrics["result"]["metrics"][12]["value"], 456);
    }
}
