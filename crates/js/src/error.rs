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
        }
    }
}

impl std::error::Error for JsError {}
