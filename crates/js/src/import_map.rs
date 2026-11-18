//! Import maps (subset of the HTML standard's import maps): exact and prefix mappings plus
//! `null` fallbacks. The map is installed with [`crate::JsRuntime::set_import_map_json`] and
//! consulted **before** the host module resolver (`set_module_resolver`), so a bare specifier
//! can be remapped to a URL without extra host plumbing.
//!
//! Supported shape:
//! ```json
//! {"imports":{"react":"https://cdn.example/react.js","utils/":"https://cdn.example/utils/",
//!             "blocked":null}}
//! ```
//! Exact keys are matched first, then the **longest prefix** ending in `/`; a `null` target
//! blocks the specifier ([`JsError::ImportMapBlocked`]). `scopes` is ignored (documented
//! subset); relative values are used verbatim (callers should pass absolute URLs). Invalid
//! JSON or non-object roots are typed errors. `integrity` metadata is not consumed here: the
//! `render` loader enforces Subresource Integrity (`megi_net::verify_integrity`, #1527) on `<script
//! integrity=…>` elements before execution.

use std::collections::BTreeMap;

use crate::error::JsError;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct MappingTable {
    exact: BTreeMap<String, Option<String>>,
    prefixes: BTreeMap<String, Option<String>>,
}

impl MappingTable {
    fn insert(&mut self, specifier: String, target: Option<String>) {
        if specifier.ends_with('/') {
            self.prefixes.insert(specifier, target);
        } else {
            self.exact.insert(specifier, target);
        }
    }

    fn resolve(&self, specifier: &str) -> Option<Option<String>> {
        if let Some(target) = self.exact.get(specifier) {
            return Some(target.clone());
        }
        let mut best: Option<(&String, &Option<String>)> = None;
        for (prefix, target) in &self.prefixes {
            if specifier.starts_with(prefix)
                && best.is_none_or(|(current, _)| prefix.len() > current.len())
            {
                best = Some((prefix, target));
            }
        }
        best.map(|(prefix, target)| {
            target
                .as_ref()
                .map(|base| format!("{base}{}", &specifier[prefix.len()..]))
        })
    }
}

fn parse_mapping_table(
    entries: &[(String, Json)],
    max_entries: usize,
) -> Result<MappingTable, JsError> {
    if entries.len() > max_entries {
        return Err(JsError::ImportMap(format!(
            "too many import-map entries (max {max_entries})"
        )));
    }
    let mut table = MappingTable::default();
    for (specifier, target) in entries {
        let target = match target {
            Json::Null => None,
            Json::String(value) => Some(value.clone()),
            _ => {
                return Err(JsError::ImportMap(format!(
                    "mapping for `{specifier}` must be a string or null"
                )));
            }
        };
        table.insert(specifier.clone(), target);
    }
    Ok(table)
}

/// Parsed import map supporting top-level `imports`, longest-prefix `scopes`, and `integrity`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportMap {
    imports: MappingTable,
    scopes: BTreeMap<String, MappingTable>,
    integrity: BTreeMap<String, String>,
}

impl ImportMap {
    /// Parses an import map from its JSON source.
    pub fn from_json(source: &str) -> Result<Self, JsError> {
        let value = Json::parse(source)
            .map_err(|error| JsError::ImportMap(format!("invalid JSON: {error}")))?;
        let root = value
            .as_object()
            .ok_or_else(|| JsError::ImportMap("root is not an object".to_string()))?;
        let mut map = Self::default();
        const MAX_IMPORTS: usize = 1024;
        if let Some(imports) = root
            .iter()
            .find(|(key, _)| key == "imports")
            .map(|(_, value)| value)
        {
            let imports = imports
                .as_object()
                .ok_or_else(|| JsError::ImportMap("`imports` is not an object".to_string()))?;
            map.imports = parse_mapping_table(imports, MAX_IMPORTS)?;
        }
        if let Some(scopes) = root
            .iter()
            .find(|(key, _)| key == "scopes")
            .map(|(_, value)| value)
        {
            let scopes = scopes
                .as_object()
                .ok_or_else(|| JsError::ImportMap("`scopes` is not an object".to_string()))?;
            if scopes.len() > MAX_IMPORTS {
                return Err(JsError::ImportMap(format!(
                    "too many import-map scopes (max {MAX_IMPORTS})"
                )));
            }
            for (scope_prefix, spec_map) in scopes {
                let entries = spec_map.as_object().ok_or_else(|| {
                    JsError::ImportMap(format!("scope `{scope_prefix}` must be an object"))
                })?;
                map.scopes.insert(
                    scope_prefix.clone(),
                    parse_mapping_table(entries, MAX_IMPORTS)?,
                );
            }
        }
        if let Some(integrity) = root
            .iter()
            .find(|(key, _)| key == "integrity")
            .map(|(_, value)| value)
        {
            let entries = integrity
                .as_object()
                .ok_or_else(|| JsError::ImportMap("`integrity` is not an object".to_string()))?;
            if entries.len() > MAX_IMPORTS {
                return Err(JsError::ImportMap(format!(
                    "too many import-map integrity entries (max {MAX_IMPORTS})"
                )));
            }
            for (url, sri) in entries {
                let Some(sri_str) = sri.as_string() else {
                    return Err(JsError::ImportMap(format!(
                        "integrity entry for `{url}` must be a string"
                    )));
                };
                map.integrity.insert(url.clone(), sri_str.to_owned());
            }
        }
        Ok(map)
    }

    /// Resolves `specifier` with no referrer scope (top-level `imports` only):
    /// `Some(Some(url))` when mapped, `Some(None)` when explicitly blocked,
    /// `None` when the map has no entry.
    pub fn resolve(&self, specifier: &str) -> Option<Option<String>> {
        self.resolve_with_base(specifier, None)
    }

    /// Resolves `specifier` using any matching `scopes` entry (longest prefix match on
    /// `base_url`) before falling back to top-level `imports`.
    pub fn resolve_with_base(
        &self,
        specifier: &str,
        base_url: Option<&str>,
    ) -> Option<Option<String>> {
        if let Some(base) = base_url {
            let mut matching_scopes: Vec<(&str, &MappingTable)> = self
                .scopes
                .iter()
                .filter(|(scope, _)| {
                    base == scope.as_str()
                        || (scope.ends_with('/') && base.starts_with(scope.as_str()))
                })
                .map(|(scope, table)| (scope.as_str(), table))
                .collect();
            // HTML Import Maps §8.1.7.3: check longest matching scope first.
            matching_scopes.sort_by(|(a, _), (b, _)| b.len().cmp(&a.len()).then_with(|| b.cmp(a)));
            for (_, table) in matching_scopes {
                if let Some(resolved) = table.resolve(specifier) {
                    return Some(resolved);
                }
            }
        }
        self.imports.resolve(specifier)
    }

    /// Returns the integrity metadata string associated with `url`, if any (#1527).
    pub fn integrity_for(&self, url: &str) -> Option<&str> {
        self.integrity.get(url).map(String::as_str)
    }

    /// Iterates `(url, integrity_metadata)` entries declared in the import map (#1527).
    pub fn integrity_entries(&self) -> impl Iterator<Item = (&str, &str)> {
        self.integrity
            .iter()
            .map(|(url, sri)| (url.as_str(), sri.as_str()))
    }

    /// Number of exact mappings.
    pub fn exact_len(&self) -> usize {
        self.imports.exact.len()
    }

    /// Number of prefix mappings.
    pub fn prefix_len(&self) -> usize {
        self.imports.prefixes.len()
    }

    /// Number of scoped overrides.
    pub fn scopes_len(&self) -> usize {
        self.scopes.len()
    }

    /// Number of integrity mappings.
    pub fn integrity_len(&self) -> usize {
        self.integrity.len()
    }
}

// JSON parser (split verbatim from the former monolithic import_map.rs).
mod parser;

pub(crate) use parser::*;

#[cfg(test)]
mod integrity_tests {
    use super::*;

    #[test]
    fn parses_integrity_object_and_rejects_invalid_values() {
        let map = ImportMap::from_json(
            r#"{"imports":{"dep":"https://cdn.test/dep.js"},"integrity":{"https://cdn.test/dep.js":"sha384-abc"}}"#,
        )
        .expect("parse import map");
        assert_eq!(map.integrity_len(), 1);
        assert_eq!(
            map.integrity_for("https://cdn.test/dep.js"),
            Some("sha384-abc")
        );
        assert!(matches!(
            ImportMap::from_json(r#"{"integrity":"bad"}"#),
            Err(JsError::ImportMap(_))
        ));
        assert!(matches!(
            ImportMap::from_json(r#"{"integrity":{"x":123}}"#),
            Err(JsError::ImportMap(_))
        ));
    }

    #[test]
    fn resolves_scopes_by_longest_prefix_and_falls_back_to_imports() {
        let map = ImportMap::from_json(
            r#"{
                "imports": {
                    "dep": "https://cdn.test/v1/dep.js",
                    "pkg/": "https://cdn.test/v1/pkg/"
                },
                "scopes": {
                    "https://app.test/a/": {
                        "dep": "https://cdn.test/v2/dep.js",
                        "blocked": null
                    },
                    "https://app.test/a/nested/": {
                        "dep": "https://cdn.test/v3/dep.js"
                    }
                }
            }"#,
        )
        .expect("parse import map with scopes");
        assert_eq!(map.scopes_len(), 2);
        assert_eq!(
            map.resolve_with_base("dep", Some("https://app.test/other.js")),
            Some(Some("https://cdn.test/v1/dep.js".to_owned()))
        );
        assert_eq!(
            map.resolve_with_base("dep", Some("https://app.test/a/page.js")),
            Some(Some("https://cdn.test/v2/dep.js".to_owned()))
        );
        assert_eq!(
            map.resolve_with_base("dep", Some("https://app.test/a/nested/mod.js")),
            Some(Some("https://cdn.test/v3/dep.js".to_owned()))
        );
        assert_eq!(
            map.resolve_with_base("blocked", Some("https://app.test/a/page.js")),
            Some(None)
        );
        assert_eq!(
            map.resolve_with_base("pkg/sub.js", Some("https://app.test/a/page.js")),
            Some(Some("https://cdn.test/v1/pkg/sub.js".to_owned()))
        );
    }
}
