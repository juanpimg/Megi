use super::*;

use boa_engine::{Context, JsError as BoaError, JsNativeError, JsResult};

/// The shared handle stored in Boa's `HostDefined` map.
///
/// This is the single owner of the host cell: the `Context` keeps it alive, and
/// every native function borrows it from the `Context` it receives. There is no
/// thread-local "current host" and no second owner on `JsRuntime`.
pub(crate) type ContextHost = Rc<RefCell<Host>>;

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn missing_host() -> BoaError {
    JsNativeError::error()
        .with_message("no JavaScript host installed in this context")
        .into()
}

/// Installs the shared `host` handle in `context`'s `HostDefined` data.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn install_host(context: &mut Context, host: ContextHost) {
    context.insert_data(host);
}

/// Mutably borrows the host attached to `context`; re-entry is a typed error.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn host_cell(context: &mut Context) -> JsResult<RefMut<'_, Host>> {
    let cell = context
        .host_defined_mut()
        .get_mut::<ContextHost>()
        .ok_or_else(missing_host)?;
    cell.try_borrow_mut().map_err(|error| {
        JsNativeError::error()
            .with_message(format!("JavaScript host is already borrowed: {error}"))
            .into()
    })
}

/// Immutably borrows the host attached to `context`.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn host_ref(context: &Context) -> JsResult<Ref<'_, Host>> {
    let cell = context.get_data::<ContextHost>().ok_or_else(missing_host)?;
    cell.try_borrow().map_err(|error| {
        JsNativeError::error()
            .with_message(format!(
                "JavaScript host is already mutably borrowed: {error}"
            ))
            .into()
    })
}
