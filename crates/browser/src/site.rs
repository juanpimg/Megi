//! Pure per-site isolation policy (ADR-004 / F7-ISOLATION, phase 1).
//!
//! A `SiteKey` is the effective origin used to decide which tabs may share a renderer.
//! The policy is deterministic and side-effect free: no window, no I/O, no global state.
//! Phase 2 (renderer pool) and phase 3 (OS processes) build on this layer; they are not
//! implemented here.

use std::collections::HashMap;

/// Effective isolation key for a URL (ADR-004).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SiteKey {
    /// A network origin: lowercase scheme + host + effective port.
    Origin {
        scheme: &'static str,
        host: String,
        port: u16,
    },
    /// An opaque origin group (`data:`, `about:`, `blob:`, `file:`, parse errors).
    /// The label lets inherited opaque origins be modelled explicitly.
    Opaque(&'static str),
}

/// Derives the isolation key for `url` (ADR-004 phase 1).
pub fn site_key(url: &str) -> SiteKey {
    let Some(parsed) = megi_webview::net::Url::parse(url).ok() else {
        return SiteKey::Opaque("invalid");
    };
    let scheme = parsed.scheme();
    match scheme {
        megi_webview::net::Scheme::Http | megi_webview::net::Scheme::Https => SiteKey::Origin {
            scheme: scheme.as_str(),
            host: parsed.host().to_ascii_lowercase(),
            port: parsed
                .port_or_default()
                .unwrap_or(if scheme.as_str() == "https" { 443 } else { 80 }),
        },
        megi_webview::net::Scheme::File => SiteKey::Opaque("file"),
        megi_webview::net::Scheme::Data => SiteKey::Opaque("data"),
    }
}

/// True when two tabs with these keys may share a renderer (same effective origin).
pub fn can_share(a: &SiteKey, b: &SiteKey) -> bool {
    a == b
}

/// Assigns stable renderer-group indices to URLs by effective site (ADR-004 phase 1).
#[derive(Debug, Default)]
pub struct SiteGroups {
    groups: Vec<SiteKey>,
    last: HashMap<String, usize>,
}

impl SiteGroups {
    pub fn new() -> Self {
        Self::default()
    }

    /// Group index for `url`, reusing an existing group for the same site.
    pub fn group_for(&mut self, url: &str) -> usize {
        if let Some(index) = self.last.get(url) {
            return *index;
        }
        let key = site_key(url);
        if let Some(index) = self.groups.iter().position(|existing| existing == &key) {
            self.last.insert(url.to_owned(), index);
            return index;
        }
        let index = self.groups.len();
        self.groups.push(key);
        self.last.insert(url.to_owned(), index);
        index
    }

    /// Number of distinct renderer groups allocated so far.
    pub fn len(&self) -> usize {
        self.groups.len()
    }

    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    /// Key of a group, for diagnostics/tests.
    pub fn key(&self, index: usize) -> Option<&SiteKey> {
        self.groups.get(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_origins_are_case_and_default_port_normalized() {
        assert_eq!(
            site_key("https://Example.COM/"),
            site_key("HTTPS://example.com:443/a")
        );
        assert_eq!(
            site_key("http://example.com/"),
            site_key("http://example.com:80/a")
        );
        assert_ne!(
            site_key("http://example.com/"),
            site_key("https://example.com/"),
            "scheme difference must isolate"
        );
        assert_ne!(
            site_key("http://example.com:8080/"),
            site_key("http://example.com/"),
            "explicit non-default port must isolate"
        );
        assert_ne!(
            site_key("http://a.example/"),
            site_key("http://b.example/"),
            "different hosts must isolate"
        );
    }

    #[test]
    fn opaque_origins_are_grouped_by_kind() {
        assert_eq!(site_key("data:text/html,hi"), SiteKey::Opaque("data"));
        assert_eq!(site_key("file:///tmp/a"), SiteKey::Opaque("file"));
        assert_eq!(site_key("about:blank"), SiteKey::Opaque("invalid"));
        assert_eq!(site_key("not a url"), SiteKey::Opaque("invalid"));
        assert!(!can_share(
            &site_key("data:text/html,hi"),
            &site_key("https://example.com/")
        ));
    }

    #[test]
    fn site_groups_reuse_a_group_for_the_same_site() {
        let mut groups = SiteGroups::new();
        assert_eq!(groups.group_for("https://example.com/a"), 0);
        assert_eq!(groups.group_for("https://example.com/b"), 0);
        assert_eq!(groups.group_for("http://other.test/"), 1);
        assert_eq!(groups.group_for("https://example.com/c"), 0);
        assert_eq!(groups.len(), 2);
        assert_eq!(
            groups.key(0),
            Some(&SiteKey::Origin {
                scheme: "https",
                host: "example.com".to_owned(),
                port: 443,
            })
        );
    }
}
