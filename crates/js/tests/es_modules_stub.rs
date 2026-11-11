//! The ES module API also exists (as `FeatureDisabled`) in the no-default-features stub.

#[cfg(not(feature = "js"))]
#[test]
fn module_api_is_disabled_without_the_js_feature() {
    use js::JsError;
    use js::runtime::Budget;
    let mut runtime = js::JsRuntime;
    let error = runtime
        .eval_module("export const x = 1;", "main", "mem://main", &Budget)
        .expect_err("feature disabled");
    assert!(matches!(error, JsError::FeatureDisabled), "{error:?}");
}

#[cfg(feature = "js")]
#[test]
fn module_api_feature_enabled_placeholder() {
    assert!(js::JsRuntime::new().is_ok());
}
