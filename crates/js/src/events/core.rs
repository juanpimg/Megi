use super::*;

pub(crate) type NativeFn = fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue>;

/// Returns the shared host cell installed in `context` by `install_host_cell`.
///
/// The guard below needs to restore state in `Drop`, where no `&mut Context` is
/// available; cloning the handle keeps the borrow local to the drop and lets the
/// dispatch loop borrow the host normally through `host_cell`/`host_ref`.
pub(crate) fn host_handle(context: &Context) -> JsResult<Rc<RefCell<Host>>> {
    context
        .get_data::<Rc<RefCell<Host>>>()
        .cloned()
        .ok_or_else(|| js_error("no JavaScript host installed in this context"))
}

/// Clones a provider out of the immutable `HostProviders` slot, falling back to
/// the legacy host mirror while the embedding runtime has not installed the slot.
/// The returned `Arc` is owned, so callers invoke it with no active host borrow.
pub(crate) fn provider_or_mirror<T>(
    context: &mut Context,
    from_slot: impl FnOnce(&crate::providers::HostProviders) -> Option<T>,
    from_mirror: impl FnOnce(&mut Host) -> Option<T>,
) -> Option<T> {
    if let Some(slot) = providers(context)
        && let Some(value) = from_slot(slot)
    {
        return Some(value);
    }
    host_cell(context)
        .ok()
        .and_then(|mut host| from_mirror(&mut host))
}

thread_local! {
    /// Clock snapshot for the no-`Context` `now_ms()` compatibility shim. The
    /// context-aware clock helpers below refresh it; a slightly stale value is
    /// harmless because `run_due_timers` uses `host.now_ms().max(deadline)`.
    pub(crate) static CLOCK_SNAPSHOT: Cell<u64> = const { Cell::new(0) };
}

pub(crate) fn refresh_clock_snapshot(context: &Context) {
    if let Ok(host) = host_ref(context) {
        CLOCK_SNAPSHOT.with(|cell| cell.set(host.now_ms()));
    }
}

/// Restores the stacked dispatch state on every exit path, including `?` early returns
/// after a listener throws or exhausts a budget (#1472).
pub(crate) struct DispatchStateGuard {
    previous: Option<DispatchState>,
    host: Rc<RefCell<Host>>,
}

impl DispatchStateGuard {
    pub(crate) fn new(context: &Context, previous: Option<DispatchState>) -> JsResult<Self> {
        Ok(Self {
            previous,
            host: host_handle(context)?,
        })
    }
}

impl Drop for DispatchStateGuard {
    fn drop(&mut self) {
        // Never panic during unwinding: a concurrent borrow simply skips the
        // restore, matching the previous best-effort `with_host` behaviour.
        if let Ok(mut host) = self.host.try_borrow_mut() {
            host.dispatch = self.previous;
        }
    }
}

/// Target of a synthetic event dispatched from Rust.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsEventTarget {
    /// The `window` global (bubbles only within window/document capture path).
    Window,
    /// The `document` object; with `bubbles` the event continues to `window`.
    Document,
    /// A DOM node, bubbling through its ancestors when requested.
    Node(megi_core::NodeId),
}

/// Kind and initialization data for a structured UI event dispatched from Rust (#1504).
#[derive(Clone, Debug, PartialEq)]
pub enum JsUiEventKind {
    /// Plain DOM `Event` (`change`, `submit`, etc.).
    Simple,
    /// `KeyboardEvent` (`keydown`, `keyup`).
    Keyboard {
        /// Key string (`KeyboardEvent.key`, e.g. `"a"`, `"Enter"`, `"Backspace"`).
        key: String,
        /// Physical/logical key code (`KeyboardEvent.code`, e.g. `"KeyA"`, `"Enter"`).
        code: String,
        /// Control modifier state.
        ctrl_key: bool,
        /// Shift modifier state.
        shift_key: bool,
        /// Alt modifier state.
        alt_key: bool,
        /// Meta/Super modifier state.
        meta_key: bool,
        /// Key auto-repeat flag.
        repeat: bool,
    },
    /// `InputEvent` (`input`).
    Input {
        /// Inserted text data (`None` for deletions such as Backspace).
        data: Option<String>,
        /// Input operation type (`"insertText"`, `"deleteContentBackward"`, etc.).
        input_type: String,
    },
    /// `FocusEvent` (`focus`, `blur`).
    Focus,
    /// `CompositionEvent` (`compositionstart`, `compositionupdate`, `compositionend`).
    Composition {
        /// Current composition text (`CompositionEvent.data`).
        data: String,
    },
    /// `AnimationEvent` (`animationstart`, `animationiteration`, `animationend`).
    Animation {
        /// Name of the `@keyframes` rule (`AnimationEvent.animationName`).
        animation_name: String,
        /// Elapsed time in seconds (`AnimationEvent.elapsedTime`).
        elapsed_time: f64,
        /// Pseudo-element selector or `""` (`AnimationEvent.pseudoElement`).
        pseudo_element: String,
    },
    /// `TransitionEvent` (`transitionrun`, `transitionstart`, `transitionend`).
    Transition {
        /// CSS property name (`TransitionEvent.propertyName`).
        property_name: String,
        /// Elapsed time in seconds (`TransitionEvent.elapsedTime`).
        elapsed_time: f64,
        /// Pseudo-element selector or `""` (`TransitionEvent.pseudoElement`).
        pseudo_element: String,
    },
}

pub(crate) fn js_error(message: impl Into<String>) -> boa_engine::JsError {
    JsNativeError::typ().with_message(message.into()).into()
}

pub(crate) fn native(f: NativeFn) -> NativeFunction {
    NativeFunction::from_fn_ptr(f)
}

pub(crate) fn accessor_fn(
    context: &mut Context,
    f: NativeFn,
) -> boa_engine::object::builtins::JsFunction {
    FunctionObjectBuilder::new(context.realm(), native(f)).build()
}

pub(crate) fn arg_string(
    args: &[JsValue],
    index: usize,
    context: &mut Context,
) -> JsResult<String> {
    let value = args.get(index).cloned().unwrap_or_else(JsValue::undefined);
    value
        .to_string(context)
        .map(|text| text.to_std_string_escaped())
        .map_err(|error| js_error(error.to_string()))
}

pub(crate) fn prop(object: &JsObject, name: &str, context: &mut Context) -> JsResult<JsValue> {
    object
        .get(js_string!(name), context)
        .map_err(|error| js_error(error.to_string()))
}

pub(crate) fn global_get(context: &mut Context, name: &str) -> JsResult<JsValue> {
    context
        .global_object()
        .get(js_string!(name), context)
        .map_err(|error| js_error(error.to_string()))
}

pub(crate) fn global_set(context: &mut Context, name: &str, value: JsValue) {
    let global = context.global_object();
    let _ = global.set(js_string!(name), value, false, context);
}

pub(crate) fn array_push(context: &mut Context, name: &str, value: JsValue) -> JsResult<u32> {
    let array = global_get(context, name)?;
    let Some(array) = array.as_object() else {
        return Err(js_error("rooted array missing"));
    };
    let length = array
        .get(js_string!("length"), context)
        .map_err(|error| js_error(error.to_string()))?
        .as_number()
        .unwrap_or(0.0) as u32;
    array
        .set(js_string!(length.to_string()), value, false, context)
        .map_err(|error| js_error(error.to_string()))?;
    Ok(length)
}

pub(crate) fn array_get(context: &mut Context, name: &str, index: u64) -> JsResult<JsValue> {
    let array = global_get(context, name)?;
    let Some(array) = array.as_object() else {
        return Ok(JsValue::undefined());
    };
    array
        .get(js_string!(index.to_string()), context)
        .map_err(|error| js_error(error.to_string()))
}

pub(crate) fn array_put(
    context: &mut Context,
    name: &str,
    index: u64,
    value: JsValue,
) -> JsResult<()> {
    let array = global_get(context, name)?;
    let Some(array) = array.as_object() else {
        return Ok(());
    };
    array
        .set(js_string!(index.to_string()), value, false, context)
        .map(|_| ())
        .map_err(|error| js_error(error.to_string()))
}
