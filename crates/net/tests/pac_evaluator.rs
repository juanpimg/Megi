//! PAC must evaluate scripts through an injected trait, not through a JS crate dependency.
#![cfg(feature = "pac")]

use megi_net::PacEvaluator;

struct FixedEvaluator;

impl PacEvaluator for FixedEvaluator {
    fn evaluate(&self, _script: &str, _url: &str, _host: &str) -> Result<String, String> {
        Ok("DIRECT".to_owned())
    }
}

#[test]
fn pac_trait_is_public_and_injectable() {
    megi_net::set_pac_evaluator(FixedEvaluator);
    assert!(megi_net::pac_evaluator().is_some());
}
