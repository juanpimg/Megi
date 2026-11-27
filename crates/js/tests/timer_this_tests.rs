//! #2344 — timer and animation-frame callbacks receive `this === globalThis`.

#![cfg(feature = "js")]

use std::time::Duration;

use megi_js::{Budget, JsRuntime};

fn runtime() -> JsRuntime {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime.set_budget(Budget::SMALL);
    runtime
}

#[test]
fn timer_and_animation_callbacks_receive_the_global_as_this() {
    let mut runtime = runtime();
    let base = runtime.advance(Duration::ZERO).expect("pin clock");
    runtime
        .eval(
            "globalThis.timeoutThis = 'unset';
             globalThis.intervalThis = 'unset';
             globalThis.frameThis = 'unset';
             // WHATWG HTML §8.1.7.4 / §12.4.4.3: the callback this value is the
             // global object. 'use strict' keeps `this` exactly as the host passed
             // it; sloppy mode would coerce undefined to globalThis and hide #2344.
             setTimeout(function () {
                 'use strict';
                 timeoutThis = this === globalThis ? 'window' : String(this);
             }, 0);
             var intervalId = setInterval(function () {
                 'use strict';
                 intervalThis = this === globalThis ? 'window' : String(this);
                 clearInterval(intervalId);
             }, 5);
             requestAnimationFrame(function (timestamp) {
                 'use strict';
                 frameThis = this === globalThis ? 'window' : String(this);
             });",
            "this.js",
        )
        .expect("schedule");
    runtime.run_until(base + 20).expect("run timers and frames");
    assert_eq!(
        runtime
            .eval(
                "[timeoutThis, intervalThis, frameThis].join('|')",
                "this.js"
            )
            .expect("this values"),
        "window|window|window"
    );
}
