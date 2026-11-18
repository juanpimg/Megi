//! Mixed-content policy (#1528): a document delivered over `https:` must not load
//! subresources over plaintext `http:`, which would let an on-path attacker replace scripts,
//! styles or images despite the secure document. The decision is pure so it can be enforced at
//! any load boundary (initial URL and every redirect hop) and unit-tested deterministically.

use crate::error::FetchError;
use crate::url::{Scheme, Url};

/// The action a loader should take for a subresource URL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MixedContentAction {
    /// Load the URL as-is (no policy applies).
    Allow,
    /// Load the equivalent `https:` URL instead (upgradeable content on an HTTPS document).
    Upgrade(String),
    /// Refuse the request (active mixed content on an HTTPS document).
    Block,
}

/// Mixed-content policy attached to a subresource request or redirect chain (#1528).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MixedContentPolicy {
    /// Scheme of the initiator document (only [`Scheme::Https`] triggers restrictions).
    pub document_scheme: Scheme,
    /// `true` for blockable active content (scripts, styles, XHR/fetch, fonts), `false` for
    /// upgradeable passive content (images, media).
    pub active: bool,
}

impl MixedContentPolicy {
    /// Builds a mixed-content policy for `document_scheme` and content class `active`.
    pub const fn new(document_scheme: Scheme, active: bool) -> Self {
        Self {
            document_scheme,
            active,
        }
    }

    /// Evaluates the policy against `resource`.
    pub fn decide(&self, resource: &Url) -> MixedContentAction {
        decide(self.document_scheme, resource, self.active)
    }

    /// Enforces the policy on `resource`, returning either the allowed/upgraded URL or
    /// [`FetchError::MixedContentBlocked`].
    pub fn apply(&self, resource: &Url) -> Result<Url, FetchError> {
        match self.decide(resource) {
            MixedContentAction::Allow => Ok(resource.clone()),
            MixedContentAction::Upgrade(upgraded) => Url::parse(&upgraded)
                .map_err(|_| FetchError::MixedContentBlocked(resource.as_str().to_owned())),
            MixedContentAction::Block => Err(FetchError::MixedContentBlocked(
                resource.as_str().to_owned(),
            )),
        }
    }
}

/// Decides how to treat `resource` when loaded from a document with scheme `document_scheme`.
///
/// - Non-HTTPS documents have no mixed-content restrictions (`Allow`).
/// - HTTPS documents block `http:` resources that are *active* (scripts, styles, iframes, XHR…).
/// - HTTPS documents *upgrade* `http:` resources that are *passive/upgradeable* (images, media)
///   to `https:`, matching modern browsers' "upgrade insecure requests" behaviour.
/// - Non-HTTP(S) resources (`data:`, `file:`, `blob:`…) are left untouched.
pub fn decide(document_scheme: Scheme, resource: &Url, active: bool) -> MixedContentAction {
    if document_scheme != Scheme::Https || resource.scheme() != Scheme::Http {
        return MixedContentAction::Allow;
    }
    if active {
        return MixedContentAction::Block;
    }
    match Url::parse(&format!(
        "https://{}",
        resource
            .as_str()
            .strip_prefix("http://")
            .unwrap_or_default()
    )) {
        Ok(upgraded) => MixedContentAction::Upgrade(upgraded.as_str().to_owned()),
        Err(_) => MixedContentAction::Block,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_https_documents_allow_everything() {
        let http = Url::parse("http://cdn.test/x.js").expect("url");
        assert_eq!(decide(Scheme::Http, &http, true), MixedContentAction::Allow);
        assert_eq!(decide(Scheme::File, &http, true), MixedContentAction::Allow);
        assert_eq!(decide(Scheme::Data, &http, true), MixedContentAction::Allow);
    }

    #[test]
    fn https_document_blocks_active_http() {
        let script = Url::parse("http://cdn.test/app.js").expect("url");
        assert_eq!(
            decide(Scheme::Https, &script, true),
            MixedContentAction::Block
        );
        let style = Url::parse("http://cdn.test/a.css?x=1").expect("url");
        assert_eq!(
            decide(Scheme::Https, &style, true),
            MixedContentAction::Block
        );
    }

    #[test]
    fn https_document_upgrades_passive_http() {
        let image = Url::parse("http://cdn.test/pic.png?w=20").expect("url");
        assert_eq!(
            decide(Scheme::Https, &image, false),
            MixedContentAction::Upgrade("https://cdn.test/pic.png?w=20".to_owned())
        );
    }

    #[test]
    fn https_document_allows_non_http_subresources() {
        let data = Url::parse("data:image/png;base64,AAAA").expect("url");
        assert_eq!(
            decide(Scheme::Https, &data, true),
            MixedContentAction::Allow
        );
        let https = Url::parse("https://cdn.test/app.js").expect("url");
        assert_eq!(
            decide(Scheme::Https, &https, true),
            MixedContentAction::Allow
        );
    }

    #[test]
    fn policy_apply_enforces_active_and_passive() {
        let http = Url::parse("http://cdn.test/asset?x=1").expect("url");
        let active = MixedContentPolicy::new(Scheme::Https, true);
        let err = active.apply(&http).expect_err("active http must block");
        assert!(matches!(err, FetchError::MixedContentBlocked(_)));

        let passive = MixedContentPolicy::new(Scheme::Https, false);
        let upgraded = passive.apply(&http).expect("passive http must upgrade");
        assert_eq!(upgraded.as_str(), "https://cdn.test/asset?x=1");

        let insecure_doc = MixedContentPolicy::new(Scheme::Http, true);
        assert_eq!(insecure_doc.apply(&http).expect("http doc"), http);
    }
}
