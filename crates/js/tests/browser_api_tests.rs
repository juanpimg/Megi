//! Browser-facing APIs (R95): matchMedia, viewport/scroll, requestAnimationFrame,
//! `document.onerror`/`window.onerror`, readyState and console completeness.

#![cfg(feature = "js")]

use std::time::Duration;

use js::{Budget, JsError, JsRuntime, ViewportInfo};

fn runtime() -> JsRuntime {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime.set_budget(Budget::SMALL);
    runtime
}

#[test]
fn match_media_uses_the_injected_provider() {
    let mut runtime = runtime();
    runtime.set_media_provider(Box::new(|query| query.contains("min-width: 700px")));
    runtime
        .eval(
            "globalThis.small = window.matchMedia('(max-width: 600px)');
             globalThis.large = window.matchMedia('(min-width: 700px)');
             small.addListener(function () {});
             small.removeListener(function () {});",
            "media.js",
        )
        .expect("matchMedia");
    assert_eq!(
        runtime.eval("small.matches", "media.js").expect("small"),
        "false"
    );
    assert_eq!(
        runtime.eval("large.matches", "media.js").expect("large"),
        "true"
    );
    assert_eq!(
        runtime.eval("large.media", "media.js").expect("media text"),
        "(min-width: 700px)"
    );
}

#[test]
fn match_media_without_provider_never_matches() {
    let mut runtime = runtime();
    assert_eq!(
        runtime
            .eval("matchMedia('(min-width: 1px)').matches", "m.js")
            .expect("matchMedia"),
        "false"
    );
}

#[test]
fn viewport_provider_exposes_inner_size_and_dpr() {
    let mut runtime = runtime();
    runtime.set_viewport_provider(Box::new(|| ViewportInfo {
        width: 1024,
        height: 768,
        device_pixel_ratio: 2.0,
        scroll_x: 3.0,
        scroll_y: 4.0,
    }));
    assert_eq!(runtime.eval("innerWidth", "v.js").expect("width"), "1024");
    assert_eq!(runtime.eval("innerHeight", "v.js").expect("height"), "768");
    assert_eq!(runtime.eval("devicePixelRatio", "v.js").expect("dpr"), "2");
    assert_eq!(runtime.eval("pageXOffset", "v.js").expect("page x"), "3");
    assert_eq!(runtime.eval("pageYOffset", "v.js").expect("page y"), "4");
}

#[test]
fn viewport_defaults_without_provider() {
    let mut runtime = runtime();
    assert_eq!(
        runtime
            .eval("innerWidth + 'x' + innerHeight", "v.js")
            .expect("default viewport"),
        "1280x720"
    );
    assert_eq!(
        runtime
            .eval("devicePixelRatio", "v.js")
            .expect("default dpr"),
        "1"
    );
}

#[test]
fn scroll_to_and_scroll_by_update_page_offsets() {
    let mut runtime = runtime();
    runtime
        .eval(
            "window.scrollTo(10, 20);
             globalThis.afterTo = pageXOffset + ',' + pageYOffset;
             window.scrollBy(5, -5);
             globalThis.afterBy = pageXOffset + ',' + pageYOffset;
             window.scrollTo({ left: 7, top: 9 });
             globalThis.afterObject = pageXOffset + ',' + pageYOffset;",
            "scroll.js",
        )
        .expect("scroll");
    assert_eq!(runtime.eval("afterTo", "scroll.js").expect("to"), "10,20");
    assert_eq!(runtime.eval("afterBy", "scroll.js").expect("by"), "15,15");
    assert_eq!(
        runtime.eval("afterObject", "scroll.js").expect("object"),
        "7,9"
    );
    assert_eq!(runtime.scroll_position(), (7.0, 9.0));
}

#[test]
fn request_animation_frame_runs_after_16ms_with_timestamp() {
    let mut runtime = runtime();
    let base = runtime.advance(Duration::ZERO).expect("pin clock");
    runtime
        .eval(
            "globalThis.frames = [];
             globalThis.handle = requestAnimationFrame(function (timestamp) {
                 frames.push(timestamp);
             });",
            "raf.js",
        )
        .expect("rAF");
    assert_eq!(runtime.pending_animation_frames(), 1);
    runtime.run_until(base + 15).expect("15ms");
    assert_eq!(
        runtime.eval("frames.length", "raf.js").expect("frames"),
        "0"
    );
    runtime.run_until(base + 16).expect("16ms");
    assert_eq!(
        runtime.eval("frames.length", "raf.js").expect("frames"),
        "1"
    );
    assert_eq!(
        runtime
            .eval("frames[0] >= 16", "raf.js")
            .expect("timestamp"),
        "true"
    );
    assert_eq!(runtime.pending_animation_frames(), 0);
}

#[test]
fn cancel_animation_frame_removes_the_callback() {
    let mut runtime = runtime();
    let base = runtime.advance(Duration::ZERO).expect("pin");
    runtime
        .eval(
            "globalThis.ran = false;
             var handle = requestAnimationFrame(function () { ran = true; });
             cancelAnimationFrame(handle);",
            "raf-cancel.js",
        )
        .expect("cancel");
    assert_eq!(runtime.pending_animation_frames(), 0);
    runtime.run_until(base + 32).expect("advance");
    assert_eq!(runtime.eval("ran", "raf-cancel.js").expect("ran"), "false");
}

#[test]
fn animation_frame_can_schedule_another_frame() {
    let mut runtime = runtime();
    let base = runtime.advance(Duration::ZERO).expect("pin");
    runtime
        .eval(
            "globalThis.count = 0;
             function frame() {
                 count += 1;
                 if (count < 2) { requestAnimationFrame(frame); }
             }
             requestAnimationFrame(frame);",
            "raf-chain.js",
        )
        .expect("chain");
    runtime.run_until(base + 16).expect("first frame");
    assert_eq!(runtime.pending_animation_frames(), 1);
    runtime.run_until(base + 33).expect("second frame");
    assert_eq!(runtime.eval("count", "raf-chain.js").expect("count"), "2");
    assert_eq!(runtime.pending_animation_frames(), 0);
}

#[test]
fn ready_state_is_loading_and_host_can_set_it() {
    let mut runtime = runtime();
    runtime
        .eval("globalThis.initial = document.readyState;", "ready.js")
        .expect("readyState");
    assert_eq!(
        runtime.eval("initial", "ready.js").expect("initial"),
        "loading"
    );
    runtime.set_ready_state("complete");
    assert_eq!(runtime.ready_state(), "complete");
    assert_eq!(
        runtime
            .eval("document.readyState", "ready.js")
            .expect("after"),
        "complete"
    );
}

#[test]
fn document_onerror_receives_five_arguments_on_uncaught_eval_error() {
    let mut runtime = runtime();
    runtime
        .eval(
            "globalThis.caught = null;
             document.onerror = function (message, source, lineno, colno, error) {
                 caught = [message, source, lineno, colno, error];
                 return false;
             };",
            "onerror.js",
        )
        .expect("install onerror");
    let error = runtime
        .eval("throw new Error('boom')", "source.js")
        .expect_err("uncaught");
    assert!(matches!(error, JsError::Runtime(_)), "{error:?}");
    assert_eq!(
        runtime.eval("caught.length", "onerror.js").expect("args"),
        "5"
    );
    assert_eq!(
        runtime
            .eval("caught[0].indexOf('boom') >= 0", "onerror.js")
            .expect("message"),
        "true"
    );
    assert_eq!(
        runtime.eval("caught[1]", "onerror.js").expect("source"),
        "source.js"
    );
    let console = runtime.take_console();
    assert!(
        console
            .iter()
            .any(|entry| entry.level == "error" && entry.message.contains("boom")),
        "uncaught errors must be captured in the console: {console:?}"
    );
}

#[test]
fn window_onerror_receives_timer_exceptions() {
    let mut runtime = runtime();
    let base = runtime.advance(Duration::ZERO).expect("pin");
    runtime
        .eval(
            "globalThis.timerError = null;
             window.onerror = function (message, source) {
                 timerError = message + '@' + source;
             };
             setTimeout(function () { throw new Error('timer-boom'); }, 0);",
            "timer-onerror.js",
        )
        .expect("install timer");
    let _ = runtime.run_until(base + 1);
    let value = runtime
        .eval("timerError", "timer-onerror.js")
        .expect("timerError");
    assert!(value.contains("timer-boom"), "value={value}");
    assert!(value.contains("<timer>"), "value={value}");
}

#[test]
fn console_assert_debug_and_table_map_to_capture_levels() {
    let mut runtime = runtime();
    runtime
        .eval(
            "console.assert(true, 'hidden');
             console.assert(false, 'shown', 42);
             console.debug('dbg');
             console.table([1, 2]);
             console.info('info');",
            "console.js",
        )
        .expect("console");
    let entries = runtime.take_console();
    let levels: Vec<&str> = entries.iter().map(|entry| entry.level.as_str()).collect();
    assert!(levels.contains(&"assert"), "{levels:?}");
    assert!(levels.contains(&"debug"), "{levels:?}");
    assert!(levels.contains(&"table"), "{levels:?}");
    assert!(levels.contains(&"info"), "{levels:?}");
    assert!(
        entries
            .iter()
            .any(|entry| entry.level == "assert" && entry.message.contains("shown")),
        "{entries:?}"
    );
    assert!(
        !entries
            .iter()
            .any(|entry| entry.level == "assert" && entry.message.contains("hidden")),
        "{entries:?}"
    );
}

#[test]
fn get_computed_style_provider_covers_render_properties() {
    let mut runtime = runtime();
    let props = [
        "color",
        "background-color",
        "font-size",
        "font-family",
        "font-weight",
        "font-style",
        "width",
        "height",
        "margin-top",
        "margin-right",
        "margin-bottom",
        "margin-left",
        "padding-top",
        "padding-right",
        "padding-bottom",
        "padding-left",
        "display",
        "position",
        "text-align",
        "line-height",
        "border-top-width",
        "border-top-color",
        "border-top-style",
        "opacity",
    ];
    runtime.set_computed_style_provider(Box::new(move |_document, _node| {
        props
            .iter()
            .map(|name| ((*name).to_string(), "1px".to_string()))
            .collect()
    }));
    runtime
        .eval(
            "var el = document.createElement('div');
             document.body.appendChild(el);
             globalThis.computed = window.getComputedStyle(el);",
            "computed.js",
        )
        .expect("getComputedStyle");
    for name in [
        "color",
        "font-size",
        "display",
        "opacity",
        "border-top-width",
    ] {
        let value = runtime
            .eval(
                &format!("computed.getPropertyValue('{name}')"),
                "computed.js",
            )
            .expect("property");
        assert_eq!(value, "1px", "{name}");
    }
    assert_eq!(
        runtime
            .eval("computed.length", "computed.js")
            .expect("length"),
        "24"
    );
}
