use super::*;

// Binding methods (split verbatim from the former monolithic api.rs).
mod core;
mod runtime_impl;

#[cfg(test)]
mod context_bridge_migration_tests {
    use super::*;

    /// A native binding must resolve its host solely from the Boa `Context`.
    /// There is no thread-local fallback any more, so this test guards the
    /// context-installed bridge used by every realm (including nested ones).
    #[test]
    pub(crate) fn binding_reads_host_from_context() {
        let mut runtime = JsRuntime::new().unwrap();
        runtime.set_title("from-context");
        let value = document_title_get(&JsValue::undefined(), &[], &mut runtime.context).unwrap();
        let text = value
            .to_string(&mut runtime.context)
            .unwrap()
            .to_std_string_escaped();
        assert_eq!(text, "from-context");
    }

    /// The adapter must drop its `RefMut` as soon as the closure returns:
    /// two sequential borrows have to succeed without a typed re-entry error.
    #[test]
    pub(crate) fn context_host_borrow_is_released_when_closure_returns() {
        let mut runtime = JsRuntime::new().unwrap();
        let before = with_context_host(&mut runtime.context, |host| host.title.clone()).unwrap();
        let after = with_context_host(&mut runtime.context, |host| {
            host.title = "second-borrow".into();
            host.title.clone()
        })
        .unwrap();
        assert_eq!(before, "");
        assert_eq!(after, "second-borrow");
    }
}
