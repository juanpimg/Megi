//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;

/// Maximum selector-match steps for one element resolution (cascade) or one top-level
/// match (direct API use).
pub const MAX_MATCH_STEPS: u32 = 200_000;

/// Maximum selector-match steps for one whole style pass over a document.
///
/// The per-element budget above is reset for every element, so a stylesheet with a
/// pathological selector grows the total work with the number of elements: 100 KB of CSS with
/// many candidate rules against 100 KB of HTML used to reach ~3.2x10^8 map operations (#1413).
/// A pass shares one budget, so the cost of resolving a document stays bounded no matter how
/// many elements it has.
pub const MAX_PASS_MATCH_STEPS: u32 = 50_000_000;

/// Maximum nesting depth of functional pseudo-class arguments while parsing: `:is(:is(…))`.
///
/// The selector grammar is recursive (`:is()`/`:where()`/`:not()` arguments are selector
/// lists), so a stylesheet of `:is(` repeated tens of thousands of times used to overflow the
/// host stack before any declaration was validated (#376).
pub const MAX_SELECTOR_NESTING: usize = 32;

thread_local! {
    pub(crate) static MATCH_STEPS: Cell<u32> = const { Cell::new(0) };
    pub(crate) static MATCH_DEPTH: Cell<u32> = const { Cell::new(0) };
    pub(crate) static MATCH_PASS_DEPTH: Cell<u32> = const { Cell::new(0) };
    pub(crate) static MATCH_EXHAUSTED: Cell<bool> = const { Cell::new(false) };
    pub(crate) static PARSER_NESTING: Cell<usize> = const { Cell::new(0) };
}

/// Tracks the [`MAX_SELECTOR_NESTING`] recursion depth of [`parse_selectors`].
pub(crate) struct ParserNestingGuard;

impl ParserNestingGuard {
    /// Returns `None` once the nesting limit is reached, so the caller can reject the
    /// selector instead of recursing further.
    pub(crate) fn enter() -> Option<Self> {
        PARSER_NESTING.with(|depth| {
            if depth.get() >= MAX_SELECTOR_NESTING {
                None
            } else {
                depth.set(depth.get() + 1);
                Some(ParserNestingGuard)
            }
        })
    }
}

impl Drop for ParserNestingGuard {
    fn drop(&mut self) {
        PARSER_NESTING.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

pub(crate) struct MatchDepthGuard;

impl Drop for MatchDepthGuard {
    fn drop(&mut self) {
        MATCH_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

pub(crate) fn with_match_budget(f: impl FnOnce() -> bool) -> bool {
    let outermost = MATCH_DEPTH.with(|depth| {
        let current = depth.get();
        depth.set(current + 1);
        current == 0
    });
    let _guard = MatchDepthGuard;
    if outermost {
        MATCH_STEPS.with(|steps| steps.set(MAX_MATCH_STEPS));
        MATCH_EXHAUSTED.with(|flag| flag.set(false));
    }
    let result = f();
    if outermost && MATCH_EXHAUSTED.with(|flag| flag.get()) {
        return false;
    }
    result
}

pub(crate) struct MatchScope {
    pub(crate) _guard: MatchDepthGuard,
}

pub(crate) fn match_scope() -> MatchScope {
    MATCH_DEPTH.with(|depth| depth.set(depth.get() + 1));
    // Only reset the step budget outside a style pass: a per-element reset is exactly the
    // quadratic blow-up #1413 describes.
    if MATCH_PASS_DEPTH.with(|depth| depth.get()) == 0 {
        MATCH_STEPS.with(|steps| steps.set(MAX_MATCH_STEPS));
        MATCH_EXHAUSTED.with(|flag| flag.set(false));
    }
    MatchScope {
        _guard: MatchDepthGuard,
    }
}

/// Scope for resolving the styles of a whole document: every element resolution inside it
/// shares one [`MAX_PASS_MATCH_STEPS`] budget instead of resetting it per element.
///
/// Nesting is allowed and only the outermost scope installs the budget, so wrapping an
/// already-scoped resolution (the renderer styles a document in several passes) is harmless.
pub struct StylePassScope {
    pub(crate) _guard: MatchPassGuard,
}

pub(crate) struct MatchPassGuard;

impl Drop for MatchPassGuard {
    fn drop(&mut self) {
        MATCH_PASS_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

/// Returns a nested style-pass scope; see [`StylePassScope`].
#[must_use]
pub fn style_pass_scope() -> StylePassScope {
    let outermost = MATCH_PASS_DEPTH.with(|depth| {
        let current = depth.get();
        depth.set(current + 1);
        current == 0
    });
    if outermost {
        MATCH_STEPS.with(|steps| steps.set(MAX_PASS_MATCH_STEPS));
        MATCH_EXHAUSTED.with(|flag| flag.set(false));
    }
    StylePassScope {
        _guard: MatchPassGuard,
    }
}

pub(crate) fn spend_match_step() -> bool {
    MATCH_STEPS.with(|steps| {
        let remaining = steps.get();
        if remaining == 0 {
            MATCH_EXHAUSTED.with(|flag| flag.set(true));
            false
        } else {
            steps.set(remaining - 1);
            true
        }
    })
}

/// Operator for attribute selectors (`[attr=value]`, `[attr~=value]`, etc.).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttrOperator {
    /// Exact equality `[attr=val]`
    Exact,
    /// Whitespace-separated word list `[attr~=val]`
    Includes,
    /// Dash-separated match `[attr|=val]` (`val` or `val-*`)
    DashMatch,
    /// Prefix match `[attr^=val]`
    Prefix,
    /// Suffix match `[attr$=val]`
    Suffix,
    /// Substring match `[attr*=val]`
    Substring,
}

/// An attribute selector requirement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttributeSelector {
    pub name: String,
    pub operator: Option<AttrOperator>,
    pub value: Option<String>,
    pub case_insensitive: bool,
}

impl AttributeSelector {
    pub fn matches(&self, doc: &Document, node_id: NodeId) -> bool {
        with_match_budget(|| self.matches_inner(doc, node_id))
    }

    pub(crate) fn matches_inner(&self, doc: &Document, node_id: NodeId) -> bool {
        let Some(el) = doc.node(node_id).as_element() else {
            return false;
        };
        let attr_val = el
            .attributes
            .iter()
            .find(|a| a.local_name.eq_ignore_ascii_case(&self.name))
            .map(|a| a.value.as_str());

        let Some(val) = attr_val else {
            return false;
        };

        let Some(op) = self.operator else {
            return true; // Simple existence check `[attr]`
        };

        let expected = self.value.as_deref().unwrap_or_default();
        let eq = |a: &str, b: &str| {
            if self.case_insensitive {
                a.eq_ignore_ascii_case(b)
            } else {
                a == b
            }
        };

        match op {
            AttrOperator::Exact => eq(val, expected),
            AttrOperator::Includes => val.split_ascii_whitespace().any(|word| eq(word, expected)),
            AttrOperator::DashMatch => {
                eq(val, expected)
                    || (val.len() > expected.len()
                        && val.as_bytes().get(expected.len()) == Some(&b'-')
                        && eq(&val[..expected.len()], expected))
            }
            AttrOperator::Prefix => {
                if self.case_insensitive {
                    val.to_ascii_lowercase()
                        .starts_with(&expected.to_ascii_lowercase())
                } else {
                    val.starts_with(expected)
                }
            }
            AttrOperator::Suffix => {
                if self.case_insensitive {
                    val.to_ascii_lowercase()
                        .ends_with(&expected.to_ascii_lowercase())
                } else {
                    val.ends_with(expected)
                }
            }
            AttrOperator::Substring => {
                if self.case_insensitive {
                    val.to_ascii_lowercase()
                        .contains(&expected.to_ascii_lowercase())
                } else {
                    val.contains(expected)
                }
            }
        }
    }
}

/// Formula $An+B$ for `:nth-child(An+B)` pseudo-classes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnPlusB {
    pub a: i32,
    pub b: i32,
}

impl AnPlusB {
    pub fn matches(&self, index_1_based: i32) -> bool {
        if self.a == 0 {
            return index_1_based == self.b;
        }
        // Widen to i64 so neither the subtraction nor the division can overflow or panic
        // (issue #674: `:nth-child(-1n-2147483648)` previously panicked in debug).
        let diff = index_1_based as i64 - self.b as i64;
        let a = self.a as i64;
        diff % a == 0 && diff / a >= 0
    }
}

/// Parses an An+B formula according to W3C CSS Syntax Level 3 §3.3 & Selectors Level 4 §15.
pub fn parse_an_plus_b(s: &str) -> Option<AnPlusB> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if s.eq_ignore_ascii_case("odd") {
        return Some(AnPlusB { a: 2, b: 1 });
    }
    if s.eq_ignore_ascii_case("even") {
        return Some(AnPlusB { a: 2, b: 0 });
    }

    // Integer only `B`
    if let Ok(b) = s.parse::<i32>() {
        return Some(AnPlusB { a: 0, b });
    }

    // Must contain 'n' or 'N'
    let n_idx = s.find(['n', 'N'])?;
    let (prefix, suffix) = (&s[..n_idx], &s[n_idx + 1..]);

    let a = match prefix.trim() {
        "" | "+" => 1,
        "-" => -1,
        num_str => num_str.parse::<i32>().ok()?,
    };

    let suffix_trimmed = suffix.replace(' ', "");
    let b = if suffix_trimmed.is_empty() {
        0
    } else {
        suffix_trimmed.parse::<i32>().ok()?
    };

    Some(AnPlusB { a, b })
}

/// Supported pseudo-classes in Selectors Level 4.
///
/// Structural pseudo-classes are evaluated against the DOM tree; the functional
/// `:is()`, `:where()`, `:not()` and `:has()` pseudo-classes recurse into nested selector
/// lists. Pseudo-classes that depend on interaction or document state that this engine does
/// not track yet (`:hover`, `:active`, `:focus`, `:local-link`, ...) and pseudo-elements
/// (`::before`, `::after`, ...) are parsed as [`PseudoClass::Never`]: the selector stays
/// valid but can never match, instead of over-matching every element.
///
/// `:lang(<language-range>)` matches the nearest ancestor-or-self `lang` value, with
/// case-insensitive hyphen-prefix semantics (Selectors 4 §12.1).
///
/// Not implemented yet: `:nth-col()`, attribute namespaces and the `s`/`i`
/// case-sensitivity flags of `@namespace`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PseudoClass {
    FirstChild,
    LastChild,
    OnlyChild,
    Empty,
    Root,
    NthChild(AnPlusB),
    FirstOfType,
    LastOfType,
    OnlyOfType,
    NthOfType(AnPlusB),
    NthLastChild(AnPlusB),
    NthLastOfType(AnPlusB),
    /// `:is(<selector-list>)`.
    Is(SelectorList),
    /// `:where(<selector-list>)`; matches like `:is()` but contributes zero specificity.
    Where(SelectorList),
    /// `:not(<selector-list>)`.
    Not(SelectorList),
    /// `:has(<relative-selector-list>)`; basic descendant/child/sibling support.
    Has(Vec<RelativeSelector>),
    /// `:focus` — host state or `data-megi-focused`.
    Focus,
    /// `:focus-visible` — focus-visible state or `data-megi-focus-visible`, falling back to `:focus`.
    FocusVisible,
    /// `:hover` — host state or `data-megi-hover` (no pointer in headless rendering).
    Hover,
    /// `:active` — host state or `data-megi-active` (no pointer in headless rendering).
    Active,
    /// `:checked` — `checked` attribute on form controls or `data-megi-checked`.
    Checked,
    /// `:disabled` — disabled form control (`disabled`/`data-megi-disabled`).
    Disabled,
    /// `:enabled` — form control that is not disabled.
    Enabled,
    /// `:read-only` — non-editable element or read-only form control.
    ReadOnly,
    /// `:read-write` — editable, non-read-only form control.
    ReadWrite,
    /// `:required` — form control with a `required` attribute.
    Required,
    /// `:placeholder-shown` — editable control with a placeholder and an empty value.
    PlaceholderShown,
    /// `:modal` — `<dialog open>` opened modally into the top layer (`data-megi-modal`).
    Modal,
    /// `:link` / `:any-link` — matches an element with an `href` attribute (Selectors 4 §3.6.1).
    Link,
    /// `:visited` — host history state or `data-megi-visited` (Selectors 4 §3.6.2).
    Visited,
    /// `:lang(<range>)` — nearest ancestor-or-self `lang`, hyphen-prefix match (#1523).
    Lang(String),
    /// `::backdrop` — pseudo-element behind a top-layer modal `<dialog>`.
    Backdrop,
    /// Known pseudo-class/pseudo-element whose state this engine cannot observe.
    Never,
}
