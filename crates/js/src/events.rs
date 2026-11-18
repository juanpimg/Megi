//! WP-A: EventTarget/Event/CustomEvent and the timer scheduler.
//!
//! - Event listeners live in the **host** table; callbacks are stored in the realm's
//!   rooted `__megi_listeners` array (Boa `JsValue`s must stay reachable from a GC root).
//! - Dispatch: capture listeners run window→document→ancestors→target, bubble listeners
//!   target→ancestors→document→window. `preventDefault`/`stopPropagation` state lives in
//!   the host for the duration of the dispatch.
//! - Timers: host-side scheduler with `(due_ms, insertion id)` ordering. `setTimeout` and
//!   `setInterval` accept extra arguments; callbacks/arguments are stored in the rooted
//!   `__megi_timer_callbacks`/`__megi_timer_args` arrays. The clock is injectable:
//!   real elapsed time by default, virtual after `JsRuntime::advance`/`run_until`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use boa_engine::{
    Context, JsNativeError, JsObject, JsResult, JsValue, NativeFunction, js_string,
    object::FunctionObjectBuilder,
    object::ObjectInitializer,
    object::builtins::JsArray,
    property::{Attribute, PropertyDescriptor},
};
use megi_core::NodeId;

use crate::error::JsError as CrateJsError;
use crate::host::{
    DispatchState, Host, InlineHandler, Listener, TargetKey, TimerEntry, ViewportInfo, host_cell,
    host_ref,
};
use crate::providers::providers;

// EventTarget/Event/CustomEvent and timer bindings (split verbatim from the former monolithic events.rs).
mod computed_style;
mod core;
mod dispatch;
mod event;
mod inline;
mod install;
mod listeners;
mod timers;
mod window;

pub(crate) use computed_style::*;
pub use core::*;
pub(crate) use dispatch::*;
pub(crate) use event::*;
pub(crate) use inline::*;
pub(crate) use install::*;
pub(crate) use listeners::*;
pub(crate) use timers::*;
pub(crate) use window::*;

#[cfg(test)]
mod dispatch_state_guard_tests {
    use super::*;
    use crate::host::install_host;
    use megi_core::Document;

    fn context_for_guard_test() -> Context {
        let mut context = Context::default();
        install_host(
            &mut context,
            Rc::new(RefCell::new(Host::new(
                Document::new(),
                "about:blank".into(),
            ))),
        );
        context
    }

    #[test]
    fn guard_restores_stacked_dispatch_state() {
        let outer = DispatchState {
            prevent_default: true,
            stop_propagation: true,
            stop_immediate: false,
            passive: true,
        };
        let mut context = context_for_guard_test();
        host_cell(&mut context).unwrap().dispatch = Some(outer);
        {
            let previous = host_cell(&mut context).unwrap().dispatch.take();
            let _guard = DispatchStateGuard::new(&context, previous).unwrap();
        }
        let restored = host_cell(&mut context).unwrap().dispatch.unwrap();
        assert!(restored.prevent_default);
        assert!(restored.stop_propagation);
        assert!(!restored.stop_immediate);
        assert!(restored.passive);
    }

    #[test]
    fn guard_without_host_is_a_typed_error() {
        let context = Context::default();
        assert!(DispatchStateGuard::new(&context, None).is_err());
    }
}
