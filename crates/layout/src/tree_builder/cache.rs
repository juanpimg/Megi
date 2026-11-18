use super::*;

/// Builds the layout tree and reports whether any box establishes `overflow` clipping.
///
/// The information lets `layout_document` skip the clip-propagation post-pass for
/// documents without clipping boxes (the common case).
/// Per-layout-call cache that interns equal `ComputedStyle` values so that the
/// 28k boxes of a text-heavy document share the (few hundred) distinct styles
/// instead of deep-cloning one style per element. Equality is always checked on a
/// bucket hit, so the key (a cached value fingerprint) can never change semantics.
/// One cached anonymous-style derivation, keyed by the parent's value fingerprint.
struct AnonymousStyle {
    /// The parent style whose derivation this is (kept to verify fingerprint collisions).
    parent: Arc<ComputedStyle>,
    derived: Arc<ComputedStyle>,
}

#[derive(Default)]
pub(super) struct StyleCache {
    buckets: HashMap<u64, Vec<Arc<ComputedStyle>>>,
    anonymous: HashMap<u64, Vec<AnonymousStyle>>,
}

impl StyleCache {
    pub(super) fn intern(&mut self, style: &ComputedStyle) -> Arc<ComputedStyle> {
        // Styles produced by the cascade carry their value fingerprint (computed once in
        // `ComputedStyle::compute`), so interning does not re-hash the whole style per
        // element. Hand-built styles (tests, `initial()`: fingerprint `0`) fall back to
        // computing it on demand so equal hand-built styles still share.
        let cached = style.fingerprint();
        let key = if cached == 0 {
            style.compute_fingerprint()
        } else {
            cached
        };
        if let Some(bucket) = self.buckets.get(&key) {
            for candidate in bucket {
                if candidate.as_ref() == style {
                    return Arc::clone(candidate);
                }
            }
        }
        let arc = Arc::new(style.clone());
        self.buckets.entry(key).or_default().push(Arc::clone(&arc));
        arc
    }

    /// Anonymous (generated-box) style derived from a parent style. Parents that
    /// share an interned style share the derived anonymous style too.
    pub(super) fn anonymous_for(&mut self, parent: &Arc<ComputedStyle>) -> Arc<ComputedStyle> {
        // Key by the parent's value fingerprint, not its Arc address (#1156): distinct
        // allocations with equal styles must share the derived anonymous style. Equality
        // is checked on a bucket hit, so a fingerprint collision cannot change semantics.
        let cached = parent.fingerprint();
        let key = if cached == 0 {
            parent.compute_fingerprint()
        } else {
            cached
        };
        if let Some(bucket) = self.anonymous.get(&key) {
            for entry in bucket {
                if entry.parent.as_ref() == parent.as_ref() {
                    return Arc::clone(&entry.derived);
                }
            }
        }
        let derived = Arc::new(anonymous_style_from(parent));
        self.anonymous.entry(key).or_default().push(AnonymousStyle {
            parent: Arc::clone(parent),
            derived: Arc::clone(&derived),
        });
        derived
    }
}

#[derive(Default)]
pub(super) struct CounterScope {
    stack: Vec<(String, i32, usize)>,
}

impl CounterScope {
    pub(super) fn apply(&mut self, style: &ComputedStyle, depth: usize) {
        for (name, initial) in &style.counter_reset {
            self.stack.push((name.clone(), *initial, depth));
        }
        for (name, delta) in &style.counter_increment {
            if let Some((_, val, _)) = self
                .stack
                .iter_mut()
                .rev()
                .find(|(existing, _, _)| existing == name)
            {
                *val = val.saturating_add(*delta);
            } else {
                self.stack.push((name.clone(), *delta, depth));
            }
        }
    }

    pub(super) fn pop_deeper_than(&mut self, depth: usize) {
        self.stack
            .retain(|(_, _, entry_depth)| *entry_depth <= depth);
    }

    pub(super) fn snapshot(&self) -> HashMap<String, i32> {
        let mut map = HashMap::new();
        for (name, value, _) in &self.stack {
            map.insert(name.clone(), *value);
        }
        map
    }
}

/// Default intrinsic size of a replaced element per HTML/CSS 2.2 conventions.
pub(super) fn replaced_default_size(tag: &str) -> (f32, f32) {
    match tag {
        // CSS 2.2 §10.3.2: a replaced element with no intrinsic dimensions uses 300x150.
        "img" | "canvas" | "video" | "object" | "embed" | "iframe" => (300.0, 150.0),
        "input" | "select" => (20.0, 20.0),
        "textarea" => (160.0, 40.0),
        "audio" => (300.0, 30.0),
        _ => (300.0, 150.0),
    }
}

#[cfg(test)]
mod style_cache_tests {
    use super::*;

    #[test]
    fn equal_parent_styles_share_anonymous_derivation() {
        let mut cache = StyleCache::default();
        // Two distinct Arc allocations holding equal styles.
        let a = Arc::new(megi_css::ComputedStyle::initial());
        let b = Arc::new(megi_css::ComputedStyle::initial());
        assert!(!Arc::ptr_eq(&a, &b));

        let derived_a = cache.anonymous_for(&a);
        let derived_b = cache.anonymous_for(&b);
        assert!(
            Arc::ptr_eq(&derived_a, &derived_b),
            "equally-styled parents must share the derived anonymous style (#1156)"
        );
    }
}
