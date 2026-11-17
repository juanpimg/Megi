//! Typed errors for the Megi JavaScript runtime.

use std::fmt;

/// Everything that can go wrong evaluating JavaScript or using the DOM bindings.
#[derive(Debug)]
pub enum JsError {
    /// The crate was built without the `js` feature (`--no-default-features`).
    FeatureDisabled,
    /// The script did not parse (`SyntaxError`).
    Syntax(String),
    /// A JavaScript exception escaped `eval`.
    Runtime(String),
    /// The configured anti-DoS budget was exhausted (loop iterations, recursion or
    /// Boa runtime limits).
    Budget(String),
    /// A DOM operation failed (stale `NodeId`, hierarchy violation, ...).
    Dom(String),
    /// Internal bridge failure (missing host context, GC/runtime setup).
    Internal(String),
    /// ES module resolution failed (specifier could not be mapped to a URL).
    ModuleNotFound(String),
    /// ES module syntax/export shape not supported by the subset preprocessor.
    ModuleSyntax(String),
    /// A requested import is not exported by the dependency module.
    ModuleExport(String),
    /// Circular ES module dependency (documented limitation).
    ModuleCycle(String),
    /// The static module graph nests deeper than the supported maximum.
    ModuleDepth(String),
    /// The static module graph exceeds the supported module count.
    ModuleBudget(String),
    /// Invalid import map (JSON syntax or unsupported shape).
    ImportMap(String),
    /// The import map explicitly maps the specifier to `null` (blocked in browsers).
    ImportMapBlocked(String),
    /// Dynamic `import()` with a non-literal argument (subset limitation).
    DynamicImport(String),
    /// Top-level `await` is not supported by the synchronous module evaluator.
    TopLevelAwait(String),
}

impl fmt::Display for JsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FeatureDisabled => write!(
                f,
                "the JavaScript runtime is disabled (build with the `js` feature)"
            ),
            Self::Syntax(message) => write!(f, "SyntaxError: {message}"),
            Self::Runtime(message) => write!(f, "JavaScript error: {message}"),
            Self::Budget(message) => write!(f, "JavaScript budget exceeded: {message}"),
            Self::Dom(message) => write!(f, "DOM error: {message}"),
            Self::Internal(message) => write!(f, "JavaScript runtime error: {message}"),
            Self::ModuleNotFound(message) => write!(f, "module not found: {message}"),
            Self::ModuleSyntax(message) => write!(f, "module syntax error: {message}"),
            Self::ModuleExport(message) => write!(f, "module export missing: {message}"),
            Self::ModuleCycle(message) => write!(f, "circular module dependency: {message}"),
            Self::ModuleDepth(message) => write!(f, "module graph too deep: {message}"),
            Self::ModuleBudget(message) => write!(f, "module graph too large: {message}"),
            Self::ImportMap(message) => write!(f, "import map error: {message}"),
            Self::ImportMapBlocked(message) => write!(f, "import map blocked: {message}"),
            Self::DynamicImport(message) => write!(f, "dynamic import error: {message}"),
            Self::TopLevelAwait(message) => write!(f, "top-level await unsupported: {message}"),
        }
    }
}

impl std::error::Error for JsError {}
