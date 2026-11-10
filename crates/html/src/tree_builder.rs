//! # WHATWG HTML §13.2.6 Tree Construction & §13.4 Fragment Parsing
//!
//! Implements the 21 Insertion Modes of the modern WHATWG HTML Living Standard
//! (§13.2.6.4.1 through §13.2.6.4.21), the Stack of Open Elements with all 5 scope
//! checks (§13.2.4.2), the List of Active Formatting Elements (§13.2.4.3), Foster
//! Parenting (§13.2.6.1), the Adoption Agency Algorithm (§13.2.6.4.7), `<template>`
//! contents insertion, and Foreign Content (SVG/MathML) attribute/tag adjustments (§13.2.6.5).

use dom::{Attribute, Document, Namespace, NodeId, NodeKind, QuirksMode};

use crate::fast_hash::{FastMap, FastSet};
use crate::scripting::{ScriptExecution, ScriptExecutor};
use crate::tokenizer::{Token, Tokenizer, TokenizerConfig, TokenizerState};

/// All 21 insertion modes of the WHATWG HTML Living Standard (§13.2.6.4.1 – §13.2.6.4.21).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum InsertionMode {
    /// 13.2.6.4.1 The "initial" insertion mode
    #[default]
    Initial,
    /// 13.2.6.4.2 The "before html" insertion mode
    BeforeHtml,
    /// 13.2.6.4.3 The "before head" insertion mode
    BeforeHead,
    /// 13.2.6.4.4 The "in head" insertion mode
    InHead,
    /// 13.2.6.4.5 The "in head noscript" insertion mode
    InHeadNoscript,
    /// 13.2.6.4.6 The "after head" insertion mode
    AfterHead,
    /// 13.2.6.4.7 The "in body" insertion mode
    InBody,
    /// 13.2.6.4.8 The "text" insertion mode
    Text,
    /// 13.2.6.4.9 The "in table" insertion mode
    InTable,
    /// 13.2.6.4.10 The "in table text" insertion mode
    InTableText,
    /// 13.2.6.4.11 The "in caption" insertion mode
    InCaption,
    /// 13.2.6.4.12 The "in column group" insertion mode
    InColumnGroup,
    /// 13.2.6.4.13 The "in table body" insertion mode
    InTableBody,
    /// 13.2.6.4.14 The "in row" insertion mode
    InRow,
    /// 13.2.6.4.15 The "in cell" insertion mode
    InCell,
    /// 13.2.6.4.16 The "in template" insertion mode
    InTemplate,
    /// 13.2.6.4.17 The "after body" insertion mode
    AfterBody,
    /// 13.2.6.4.18 The "in frameset" insertion mode
    InFrameset,
    /// 13.2.6.4.19 The "after frameset" insertion mode
    AfterFrameset,
    /// 13.2.6.4.20 The "after after body" insertion mode
    AfterAfterBody,
    /// 13.2.6.4.21 The "after after frameset" insertion mode
    AfterAfterFrameset,
}

/// Entry in the List of Active Formatting Elements (§13.2.4.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormattingEntry {
    Marker,
    Element {
        node_id: NodeId,
        tag_name: String,
        attributes: Vec<(String, String)>,
    },
}

/// Insertion location determined by "appropriate place for inserting a node" (§13.2.6.1).
#[derive(Clone, Copy, Debug)]
struct AdjustedInsertionLocation {
    parent: NodeId,
    insert_before_sibling: Option<NodeId>,
}

const DEFAULT_SCOPE_ELEMENTS: &[(&str, Namespace)] = &[
    ("applet", Namespace::Html),
    ("caption", Namespace::Html),
    ("html", Namespace::Html),
    ("table", Namespace::Html),
    ("td", Namespace::Html),
    ("th", Namespace::Html),
    ("marquee", Namespace::Html),
    ("object", Namespace::Html),
    ("select", Namespace::Html),
    ("template", Namespace::Html),
    ("mi", Namespace::MathMl),
    ("mo", Namespace::MathMl),
    ("mn", Namespace::MathMl),
    ("ms", Namespace::MathMl),
    ("mtext", Namespace::MathMl),
    ("annotation-xml", Namespace::MathMl),
    ("foreignObject", Namespace::Svg),
    ("desc", Namespace::Svg),
    ("title", Namespace::Svg),
];

const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "basefont", "bgsound", "br", "col", "embed", "frame", "hr", "img", "input",
    "keygen", "link", "meta", "param", "source", "track", "wbr",
];

const FORMATTING_ELEMENTS: &[&str] = &[
    "a", "b", "big", "code", "em", "font", "i", "nobr", "s", "small", "strike", "strong", "tt", "u",
];

const IMPLIED_END_TAG_ELEMENTS: &[&str] = &[
    "dd", "dt", "li", "optgroup", "option", "p", "rb", "rp", "rt", "rtc",
];

const P_CLOSING_BLOCK_ELEMENTS: &[&str] = &[
    "address",
    "article",
    "aside",
    "blockquote",
    "center",
    "details",
    "dialog",
    "dir",
    "div",
    "dl",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hgroup",
    "hr",
    "listing",
    "main",
    "menu",
    "nav",
    "ol",
    "p",
    "pre",
    "search",
    "section",
    "summary",
    "table",
    "ul",
];

const BLOCK_END_TAG_ELEMENTS: &[&str] = &[
    "address",
    "article",
    "aside",
    "blockquote",
    "button",
    "center",
    "details",
    "dialog",
    "dir",
    "div",
    "dl",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "header",
    "hgroup",
    "listing",
    "main",
    "menu",
    "nav",
    "ol",
    "pre",
    "search",
    "section",
    "select",
    "summary",
    "ul",
];

const QUIRKS_PUBLIC_EXACT: &[&str] = &[
    "-//w3o//dtd w3 html strict 3.0//en//",
    "-/w3c/dtd html 4.0 transitional/en",
    "html",
];

const QUIRKS_PUBLIC_PREFIXES: &[&str] = &[
    "+//silmaril//dtd html pro v0r11 19970101//",
    "-//as//dtd html 3.0 aswedit + extensions//",
    "-//advasoft ltd//dtd html 3.0 aswedit + extensions//",
    "-//ietf//dtd html 2.0 level 1//",
    "-//ietf//dtd html 2.0 level 2//",
    "-//ietf//dtd html 2.0 strict level 1//",
    "-//ietf//dtd html 2.0 strict level 2//",
    "-//ietf//dtd html 2.0 strict//",
    "-//ietf//dtd html 2.0//",
    "-//ietf//dtd html 2.1e//",
    "-//ietf//dtd html 3.0//",
    "-//ietf//dtd html 3.2 final//",
    "-//ietf//dtd html 3.2//",
    "-//ietf//dtd html 3//",
    "-//ietf//dtd html level 0//",
    "-//ietf//dtd html level 1//",
    "-//ietf//dtd html level 2//",
    "-//ietf//dtd html level 3//",
    "-//ietf//dtd html strict level 0//",
    "-//ietf//dtd html strict level 1//",
    "-//ietf//dtd html strict level 2//",
    "-//ietf//dtd html strict level 3//",
    "-//ietf//dtd html strict//",
    "-//ietf//dtd html//",
    "-//metrius//dtd metrius presentational//",
    "-//microsoft//dtd internet explorer 2.0 html strict//",
    "-//microsoft//dtd internet explorer 2.0 html//",
    "-//microsoft//dtd internet explorer 2.0 tables//",
    "-//microsoft//dtd internet explorer 3.0 html strict//",
    "-//microsoft//dtd internet explorer 3.0 html//",
    "-//microsoft//dtd internet explorer 3.0 tables//",
    "-//netscape comm. corp.//dtd html//",
    "-//netscape comm. corp.//dtd strict html//",
    "-//o'reilly and associates//dtd html 2.0//",
    "-//o'reilly and associates//dtd html extended 1.0//",
    "-//o'reilly and associates//dtd html extended relaxed 1.0//",
    "-//sq//dtd html 2.0 hotmetal + extensions//",
    "-//softquad software//dtd hotmetal pro 6.0::19990601::extensions to html 4.0//",
    "-//softquad//dtd hotmetal pro 4.0::19971010::extensions to html 4.0//",
    "-//spyglass//dtd html 2.0 extended//",
    "-//sun microsystems corp.//dtd hotjava html//",
    "-//sun microsystems corp.//dtd hotjava strict html//",
    "-//w3c//dtd html 3 1995-03-24//",
    "-//w3c//dtd html 3.2 draft//",
    "-//w3c//dtd html 3.2 final//",
    "-//w3c//dtd html 3.2//",
    "-//w3c//dtd html 3.2s draft//",
    "-//w3c//dtd html 4.0 frameset//",
    "-//w3c//dtd html 4.0 transitional//",
    "-//w3c//dtd html experimental 19960712//",
    "-//w3c//dtd html experimental 970421//",
    "-//w3c//dtd w3 html//",
    "-//w3o//dtd w3 html 3.0//",
    "-//webtechs//dtd mozilla html 2.0//",
    "-//webtechs//dtd mozilla html//",
];

const LIMITED_QUIRKS_PREFIXES: &[&str] = &[
    "-//w3c//dtd xhtml 1.0 frameset//",
    "-//w3c//dtd xhtml 1.0 transitional//",
];

fn determine_doctype_quirks_mode(
    force_quirks: bool,
    name: Option<&str>,
    public_id: Option<&str>,
    system_id: Option<&str>,
) -> QuirksMode {
    if force_quirks || name.is_none() || !name.unwrap().eq_ignore_ascii_case("html") {
        return QuirksMode::Quirks;
    }

    if let Some(sys) = system_id
        && sys.eq_ignore_ascii_case("http://www.ibm.com/data/dtd/v11/ibmxhtml1-transitional.dtd")
    {
        return QuirksMode::Quirks;
    }

    if let Some(pub_id) = public_id {
        let pub_lower = pub_id.to_ascii_lowercase();
        if QUIRKS_PUBLIC_EXACT.contains(&pub_lower.as_str()) {
            return QuirksMode::Quirks;
        }
        if QUIRKS_PUBLIC_PREFIXES
            .iter()
            .any(|prefix| pub_lower.starts_with(prefix))
        {
            return QuirksMode::Quirks;
        }
        if system_id.is_none()
            && (pub_lower.starts_with("-//w3c//dtd html 4.01 frameset//")
                || pub_lower.starts_with("-//w3c//dtd html 4.01 transitional//"))
        {
            return QuirksMode::Quirks;
        }

        if LIMITED_QUIRKS_PREFIXES
            .iter()
            .any(|prefix| pub_lower.starts_with(prefix))
        {
            return QuirksMode::LimitedQuirks;
        }
        if system_id.is_some()
            && (pub_lower.starts_with("-//w3c//dtd html 4.01 frameset//")
                || pub_lower.starts_with("-//w3c//dtd html 4.01 transitional//"))
        {
            return QuirksMode::LimitedQuirks;
        }
    }

    QuirksMode::NoQuirks
}

const SVG_ELEMENT_NAME_ADJUSTMENTS: &[(&str, &str)] = &[
    ("altglyph", "altGlyph"),
    ("altglyphdef", "altGlyphDef"),
    ("altglyphitem", "altGlyphItem"),
    ("animatecolor", "animateColor"),
    ("animatemotion", "animateMotion"),
    ("animatetransform", "animateTransform"),
    ("clippath", "clipPath"),
    ("feblend", "feBlend"),
    ("fecolormatrix", "feColorMatrix"),
    ("fecomponenttransfer", "feComponentTransfer"),
    ("fecomposite", "feComposite"),
    ("feconvolvematrix", "feConvolveMatrix"),
    ("fediffuselighting", "feDiffuseLighting"),
    ("fedisplacementmap", "feDisplacementMap"),
    ("fedistantlight", "feDistantLight"),
    ("fedropshadow", "feDropShadow"),
    ("feflood", "feFlood"),
    ("fefunca", "feFuncA"),
    ("fefuncb", "feFuncB"),
    ("fefuncg", "feFuncG"),
    ("fefuncr", "feFuncR"),
    ("fegaussianblur", "feGaussianBlur"),
    ("feimage", "feImage"),
    ("femerge", "feMerge"),
    ("femergenode", "feMergeNode"),
    ("femorphology", "feMorphology"),
    ("feoffset", "feOffset"),
    ("fepointlight", "fePointLight"),
    ("fespecularlighting", "feSpecularLighting"),
    ("fespotlight", "feSpotLight"),
    ("fetile", "feTile"),
    ("feturbulence", "feTurbulence"),
    ("foreignobject", "foreignObject"),
    ("glyphref", "glyphRef"),
    ("lineargradient", "linearGradient"),
    ("radialgradient", "radialGradient"),
    ("textpath", "textPath"),
];

const SVG_ATTRIBUTE_ADJUSTMENTS: &[(&str, &str)] = &[
    ("attributename", "attributeName"),
    ("attributetype", "attributeType"),
    ("basefrequency", "baseFrequency"),
    ("baseprofile", "baseProfile"),
    ("calcmode", "calcMode"),
    ("clippathunits", "clipPathUnits"),
    ("diffuseconstant", "diffuseConstant"),
    ("edgemode", "edgeMode"),
    ("filterunits", "filterUnits"),
    ("glyphref", "glyphRef"),
    ("gradienttransform", "gradientTransform"),
    ("gradientunits", "gradientUnits"),
    ("kernelmatrix", "kernelMatrix"),
    ("kernelunitlength", "kernelUnitLength"),
    ("keypoints", "keyPoints"),
    ("keysplines", "keySplines"),
    ("keytimes", "keyTimes"),
    ("lengthadjust", "lengthAdjust"),
    ("limitingconeangle", "limitingConeAngle"),
    ("markerheight", "markerHeight"),
    ("markerunits", "markerUnits"),
    ("markerwidth", "markerWidth"),
    ("maskcontentunits", "maskContentUnits"),
    ("maskunits", "maskUnits"),
    ("numoctaves", "numOctaves"),
    ("pathlength", "pathLength"),
    ("patterncontentunits", "patternContentUnits"),
    ("patterntransform", "patternTransform"),
    ("patternunits", "patternUnits"),
    ("pointsatx", "pointsAtX"),
    ("pointsaty", "pointsAtY"),
    ("pointsatz", "pointsAtZ"),
    ("preservealpha", "preserveAlpha"),
    ("preserveaspectratio", "preserveAspectRatio"),
    ("primitiveunits", "primitiveUnits"),
    ("refx", "refX"),
    ("refy", "refY"),
    ("repeatcount", "repeatCount"),
    ("repeatdur", "repeatDur"),
    ("requiredextensions", "requiredExtensions"),
    ("requiredfeatures", "requiredFeatures"),
    ("specularconstant", "specularConstant"),
    ("specularexponent", "specularExponent"),
    ("spreadmethod", "spreadMethod"),
    ("startoffset", "startOffset"),
    ("stddeviation", "stdDeviation"),
    ("stitchtiles", "stitchTiles"),
    ("surfacescale", "surfaceScale"),
    ("systemlanguage", "systemLanguage"),
    ("tablevalues", "tableValues"),
    ("targetx", "targetX"),
    ("targety", "targetY"),
    ("textlength", "textLength"),
    ("viewbox", "viewBox"),
    ("viewtarget", "viewTarget"),
    ("xchannelselector", "xChannelSelector"),
    ("ychannelselector", "yChannelSelector"),
    ("zoomandpan", "zoomAndPan"),
];

const FOREIGN_ATTRIBUTE_ADJUSTMENTS: &[(&str, Option<&str>, &str, Namespace)] = &[
    ("xlink:actuate", Some("xlink"), "actuate", Namespace::XLink),
    ("xlink:arcrole", Some("xlink"), "arcrole", Namespace::XLink),
    ("xlink:href", Some("xlink"), "href", Namespace::XLink),
    ("xlink:role", Some("xlink"), "role", Namespace::XLink),
    ("xlink:show", Some("xlink"), "show", Namespace::XLink),
    ("xlink:title", Some("xlink"), "title", Namespace::XLink),
    ("xlink:type", Some("xlink"), "type", Namespace::XLink),
    ("xml:lang", Some("xml"), "lang", Namespace::Xml),
    ("xml:space", Some("xml"), "space", Namespace::Xml),
    ("xmlns", None, "xmlns", Namespace::Xmlns),
    ("xmlns:xlink", Some("xmlns"), "xlink", Namespace::Xmlns),
];

const FOREIGN_BREAKOUT_TAGS: &[&str] = &[
    "b",
    "big",
    "blockquote",
    "body",
    "br",
    "center",
    "code",
    "dd",
    "div",
    "dl",
    "dt",
    "em",
    "embed",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "head",
    "hr",
    "i",
    "img",
    "li",
    "listing",
    "menu",
    "meta",
    "nobr",
    "ol",
    "p",
    "pre",
    "ruby",
    "s",
    "small",
    "span",
    "strong",
    "strike",
    "sub",
    "sup",
    "table",
    "tt",
    "u",
    "ul",
    "var",
];

/// Checks if an element is in the "special" category (§13.2.4.2).
pub fn is_special_category(ns: Namespace, local_name: &str) -> bool {
    match ns {
        Namespace::Html => matches!(
            local_name,
            "address"
                | "applet"
                | "area"
                | "article"
                | "aside"
                | "base"
                | "basefont"
                | "bgsound"
                | "blockquote"
                | "body"
                | "br"
                | "button"
                | "caption"
                | "center"
                | "col"
                | "colgroup"
                | "dd"
                | "details"
                | "dir"
                | "div"
                | "dl"
                | "dt"
                | "embed"
                | "fieldset"
                | "figcaption"
                | "figure"
                | "footer"
                | "form"
                | "frame"
                | "frameset"
                | "h1"
                | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
                | "head"
                | "header"
                | "hgroup"
                | "hr"
                | "html"
                | "iframe"
                | "img"
                | "input"
                | "keygen"
                | "li"
                | "link"
                | "listing"
                | "main"
                | "marquee"
                | "menu"
                | "meta"
                | "nav"
                | "noembed"
                | "noframes"
                | "noscript"
                | "object"
                | "ol"
                | "p"
                | "param"
                | "plaintext"
                | "pre"
                | "script"
                | "search"
                | "section"
                | "select"
                | "source"
                | "style"
                | "summary"
                | "table"
                | "tbody"
                | "td"
                | "template"
                | "textarea"
                | "tfoot"
                | "th"
                | "thead"
                | "title"
                | "tr"
                | "track"
                | "ul"
                | "wbr"
                | "xmp"
        ),
        Namespace::MathMl => matches!(
            local_name,
            "mi" | "mo" | "mn" | "ms" | "mtext" | "annotation-xml"
        ),
        Namespace::Svg => matches!(local_name, "foreignObject" | "desc" | "title"),
        _ => false,
    }
}

/// Hard parsing limits applied by [`parse_document_with_options`] /
/// [`parse_fragment_with_options`].
///
/// These are **anti-DoS defenses**, not conformance knobs: real-world pages and the
/// complete `html5lib-tests` corpus stay far below every default, so enabling the
/// defaults never changes the resulting tree for conforming input. Hostile input that
/// exceeds a limit is handled by *ignoring* further insertions and counting a
/// [`LimitViolations`] entry instead of panicking, aborting or recursing without bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParseOptions {
    /// Maximum element nesting depth (length of the stack of open elements). Deeper
    /// start tags are dropped. Bounds the recursion depth of downstream consumers
    /// (layout, paint) as well.
    pub max_depth: usize,
    /// Maximum number of nodes allocated in the document arena. Further start tags,
    /// comments and text are dropped.
    pub max_nodes: usize,
    /// Maximum number of attributes kept on a single element. Excess attributes are
    /// dropped (the first ones win, matching the duplicate-attribute rule).
    pub max_attributes_per_element: usize,
    /// Maximum number of input bytes tokenized. The input is truncated on a UTF-8
    /// character boundary; a violation is recorded in the outcome.
    pub max_input_bytes: usize,
}

impl ParseOptions {
    /// Generous defaults, well above every official html5lib fixture (largest fixture is
    /// a few KB, at most a handful of attributes and nesting levels), but far below the
    /// input sizes that make the tokenizer/tree builder consume seconds of CPU or
    /// exhaust the stack.
    pub const DEFAULT: Self = Self {
        max_depth: 4096,
        max_nodes: 1_000_000,
        max_attributes_per_element: 4096,
        max_input_bytes: 16 * 1024 * 1024,
    };
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Counter of parsing limits hit while processing a document; see [`ParseOptions`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LimitViolations {
    /// The input was truncated because it exceeded [`ParseOptions::max_input_bytes`].
    pub input_truncated: bool,
    /// Number of start tags ignored because [`ParseOptions::max_depth`] was reached.
    pub depth_exceeded: u64,
    /// Number of tokens ignored because [`ParseOptions::max_nodes`] was reached.
    pub nodes_exceeded: u64,
    /// Number of attributes dropped because
    /// [`ParseOptions::max_attributes_per_element`] was exceeded.
    pub attributes_dropped: u64,
}

impl LimitViolations {
    /// Returns `true` when no limit was exceeded.
    pub const fn is_empty(&self) -> bool {
        !self.input_truncated
            && self.depth_exceeded == 0
            && self.nodes_exceeded == 0
            && self.attributes_dropped == 0
    }

    /// Total number of recorded violations (the input truncation counts as one).
    pub const fn total(&self) -> u64 {
        let truncated = self.input_truncated as u64;
        truncated + self.depth_exceeded + self.nodes_exceeded + self.attributes_dropped
    }
}

/// Result of a limited parse: the produced tree plus which (if any) safety limits fired.
#[derive(Debug)]
pub struct ParseOutcome {
    /// The parsed document (possibly truncated/downgraded by [`ParseOptions`]).
    pub document: Document,
    /// Limits that were exceeded while parsing.
    pub limit_violations: LimitViolations,
    /// Non-fatal errors from parser-inserted scripts (WP-6.4). Empty when no executor is
    /// installed, scripting is off, or every script ran successfully.
    pub script_errors: Vec<String>,
}

/// WHATWG HTML §13.2.6 Tree Builder state machine.
pub struct TreeBuilder {
    pub doc: Document,
    pub mode: InsertionMode,
    pub original_mode: InsertionMode,
    pub open_elements: Vec<NodeId>,
    pub active_formatting: Vec<FormattingEntry>,
    pub template_modes: Vec<InsertionMode>,
    pub head_element: Option<NodeId>,
    pub form_element: Option<NodeId>,
    pub scripting_enabled: bool,
    pub frameset_ok: bool,
    pub foster_parenting: bool,
    pub skip_next_lf: bool,
    pub fragment_context: Option<(Namespace, String)>,
    pub fragment_context_node: Option<NodeId>,
    pub pending_table_characters: Vec<char>,
    /// Hard safety limits for this parse (anti-DoS).
    pub parse_options: ParseOptions,
    /// Limits that have been exceeded so far.
    pub limit_violations: LimitViolations,
    /// Whether any HTML `<select>` element was inserted; lets the end-of-parse
    /// `<selectedcontent>` fixup (which walks the whole tree) be skipped entirely for
    /// the overwhelmingly common documents without a `<select>`.
    has_select_element: bool,
    /// Anti-quadratic fast path for §13.2.4.2 scope checks: number of currently-open
    /// HTML-namespace elements per local name (audit finding F2). "The tag is not open at
    /// all" answers `has_element_in_*_scope` in O(1) instead of scanning the whole stack.
    open_html_counts: FastMap<String, u32>,
    /// O(1) membership test for the stack of open elements (used by the active-formatting
    /// reconstruction, which previously scanned the stack once per character token).
    open_ids: FastSet<NodeId>,
    /// Number of currently-open non-HTML (SVG/MathML) elements. Allows the per-token
    /// foreign-content dispatch to skip all node lookups for HTML-only pages.
    open_foreign_count: u32,
    /// WP-6.4: host-provided executor for parser-inserted `<script>` elements. `None` keeps
    /// the parser byte-for-byte identical to the pre-WP-6.4 parser (scripts are not run).
    script_executor: Option<Box<dyn ScriptExecutor>>,
    /// HTML written through `document.write` by the script processed in the current token.
    /// The tokenizer driver moves it into the tokenizer right after the token is handled.
    pending_written_html: String,
    /// Non-fatal parser-inserted script errors, exposed through [`ParseOutcome::script_errors`].
    script_errors: Vec<String>,
    /// Whether this parse ever created an `<html>` element. A script may detach it
    /// (`document.documentElement.remove()`); the end-of-parse synthetic html/head/body fixup
    /// must then be skipped, or it would resurrect a detached skeleton (`scripted_foster01`).
    html_element_seen: bool,
}

impl TreeBuilder {
    pub fn new(scripting_enabled: bool) -> Self {
        Self::with_options(scripting_enabled, ParseOptions::default())
    }

    /// Creates a tree builder with explicit anti-DoS [`ParseOptions`].
    pub fn with_options(scripting_enabled: bool, parse_options: ParseOptions) -> Self {
        Self::with_document(scripting_enabled, parse_options, Document::new())
    }

    /// Creates a tree builder that parses into an existing [`Document`].
    ///
    /// Used by the in-place fragment API ([`parse_fragment_into`]) so `innerHTML`-style
    /// operations do not need to allocate a temporary document and copy the resulting
    /// subtree across arenas. The document's existing tree is preserved; the parser appends
    /// its synthetic `<html>` root as the last child of `doc.root()` and moves the fragment
    /// children there at the end.
    pub fn with_document(
        scripting_enabled: bool,
        parse_options: ParseOptions,
        doc: Document,
    ) -> Self {
        Self {
            doc,
            mode: InsertionMode::Initial,
            original_mode: InsertionMode::Initial,
            open_elements: Vec::new(),
            active_formatting: Vec::new(),
            template_modes: Vec::new(),
            head_element: None,
            form_element: None,
            scripting_enabled,
            frameset_ok: true,
            foster_parenting: false,
            skip_next_lf: false,
            fragment_context: None,
            fragment_context_node: None,
            pending_table_characters: Vec::new(),
            parse_options,
            limit_violations: LimitViolations::default(),
            has_select_element: false,
            open_html_counts: FastMap::default(),
            open_ids: FastSet::default(),
            open_foreign_count: 0,
            script_executor: None,
            pending_written_html: String::new(),
            script_errors: Vec::new(),
            html_element_seen: false,
        }
    }

    /// Installs a host-provided executor for parser-inserted scripts (WP-6.4).
    ///
    /// The parser only calls it for inline HTML `<script>` elements closed while
    /// [`Self::scripting_enabled`] is true; see [`crate::scripting`] for the documented
    /// cases and for how `document.write` output is re-tokenized in place.
    pub fn with_script_executor(mut self, executor: Box<dyn ScriptExecutor>) -> Self {
        self.script_executor = Some(executor);
        self
    }

    /// Takes the HTML written by `document.write` while handling the token just processed.
    fn take_pending_written_html(&mut self) -> Option<String> {
        if self.pending_written_html.is_empty() {
            None
        } else {
            Some(std::mem::take(&mut self.pending_written_html))
        }
    }

    /// Pushes `node_id` onto the stack of open elements, keeping the O(1) scope-check
    /// bookkeeping (`open_html_counts`, `open_ids`) in sync.
    fn push_open_element(&mut self, node_id: NodeId) {
        self.open_elements.push(node_id);
        self.open_ids.insert(node_id);
        self.adjust_open_html_count(node_id, 1);
    }

    /// Pops the topmost open element, keeping the O(1) bookkeeping in sync.
    fn pop_open_element(&mut self) -> Option<NodeId> {
        let node_id = self.open_elements.pop()?;
        self.open_ids.remove(&node_id);
        self.adjust_open_html_count(node_id, -1);
        Some(node_id)
    }

    /// Removes an open element at an arbitrary stack position (adoption agency), keeping
    /// the O(1) bookkeeping in sync.
    fn remove_open_element(&mut self, pos: usize) -> NodeId {
        let node_id = self.open_elements.remove(pos);
        self.open_ids.remove(&node_id);
        self.adjust_open_html_count(node_id, -1);
        node_id
    }

    /// Inserts an open element at an arbitrary stack position (adoption agency), keeping
    /// the O(1) bookkeeping in sync.
    fn insert_open_element(&mut self, pos: usize, node_id: NodeId) {
        self.open_elements.insert(pos, node_id);
        self.open_ids.insert(node_id);
        self.adjust_open_html_count(node_id, 1);
    }

    /// Replaces the open element at `pos` with `new_node_id` (adoption agency), keeping
    /// the O(1) bookkeeping in sync. Returns the node that was replaced.
    fn replace_open_element(&mut self, pos: usize, new_node_id: NodeId) -> NodeId {
        let old_node_id = std::mem::replace(&mut self.open_elements[pos], new_node_id);
        self.open_ids.remove(&old_node_id);
        self.open_ids.insert(new_node_id);
        self.adjust_open_html_count(old_node_id, -1);
        self.adjust_open_html_count(new_node_id, 1);
        old_node_id
    }

    /// Returns the number of currently-open HTML-namespace elements with local name `name`.
    fn open_html_count(&self, name: &str) -> u32 {
        self.open_html_counts.get(name).copied().unwrap_or(0)
    }

    /// Returns `true` when an HTML-namespace element with local name `name` is open.
    fn has_open_html_element(&self, name: &str) -> bool {
        self.open_html_count(name) > 0
    }

    /// Returns `true` when `node_id` is currently on the stack of open elements.
    fn is_open_element(&self, node_id: NodeId) -> bool {
        self.open_ids.contains(&node_id)
    }

    /// Increments/decrements the per-tag counter for open HTML-namespace elements.
    ///
    /// Non-element nodes and foreign (SVG/MathML) elements are not counted: the scope
    /// checks only ever match HTML-namespace elements by local name, so the counter is a
    /// faithful mirror of the only thing they test.
    fn adjust_open_html_count(&mut self, node_id: NodeId, delta: i32) {
        let Some(el) = self.doc.node(node_id).as_element() else {
            return;
        };
        if el.namespace != Namespace::Html {
            // Keep the foreign-content fast path in sync (see `should_dispatch_to_foreign`).
            if delta > 0 {
                self.open_foreign_count += 1;
            } else {
                self.open_foreign_count = self.open_foreign_count.saturating_sub(1);
            }
            return;
        }
        if delta > 0 {
            if let Some(count) = self.open_html_counts.get_mut(el.local_name.as_str()) {
                *count += 1;
            } else {
                self.open_html_counts.insert(el.local_name.clone(), 1);
            }
        } else {
            let name = el.local_name.as_str();
            let mut remove = false;
            if let Some(count) = self.open_html_counts.get_mut(name) {
                *count -= 1;
                remove = *count == 0;
            }
            if remove {
                self.open_html_counts.remove(name);
            }
        }
    }

    /// Current node at the bottom of the stack of open elements (§13.2.4.2).
    pub fn current_node(&self) -> Option<NodeId> {
        self.open_elements.last().copied()
    }

    fn node_is(&self, node_id: NodeId, ns: Namespace, name: &str) -> bool {
        self.doc
            .node(node_id)
            .as_element()
            .is_some_and(|el| el.namespace == ns && el.local_name == name)
    }

    fn node_in_html(&self, node_id: NodeId, name: &str) -> bool {
        self.node_is(node_id, Namespace::Html, name)
    }

    pub fn is_node_special(&self, node_id: NodeId) -> bool {
        self.doc
            .node(node_id)
            .as_element()
            .is_some_and(|el| is_special_category(el.namespace, &el.local_name))
    }

    pub fn adjusted_current_node(&self) -> Option<NodeId> {
        if self.open_elements.len() == 1
            && let Some(ctx_node) = self.fragment_context_node
        {
            Some(ctx_node)
        } else {
            self.current_node()
        }
    }

    pub fn is_adjusted_current_node_foreign(&self) -> bool {
        // Fast path: with no foreign element open (and no fragment context, whose
        // namespace could be foreign) the adjusted current node is necessarily HTML.
        if self.open_foreign_count == 0 && self.fragment_context_node.is_none() {
            return false;
        }
        if let Some(id) = self.adjusted_current_node()
            && let Some(el) = self.doc.node(id).as_element()
        {
            return el.namespace != Namespace::Html;
        }
        false
    }

    pub fn is_mathml_text_integration_point(&self, node_id: NodeId) -> bool {
        self.doc.node(node_id).as_element().is_some_and(|el| {
            el.namespace == Namespace::MathMl
                && matches!(el.local_name.as_str(), "mi" | "mo" | "mn" | "ms" | "mtext")
        })
    }

    pub fn is_html_integration_point(&self, node_id: NodeId) -> bool {
        self.doc
            .node(node_id)
            .as_element()
            .is_some_and(|el| match el.namespace {
                Namespace::MathMl if el.local_name == "annotation-xml" => {
                    el.attributes.iter().any(|attr| {
                        attr.local_name == "encoding"
                            && (attr.value.eq_ignore_ascii_case("text/html")
                                || attr.value.eq_ignore_ascii_case("application/xhtml+xml"))
                    })
                }
                Namespace::Svg => {
                    matches!(el.local_name.as_str(), "foreignObject" | "desc" | "title")
                }
                _ => false,
            })
    }

    pub fn is_integration_point_or_html(&self, node_id: NodeId) -> bool {
        self.doc.node(node_id).as_element().is_some_and(|el| {
            el.namespace == Namespace::Html
                || self.is_mathml_text_integration_point(node_id)
                || self.is_html_integration_point(node_id)
        })
    }

    /// Cheap conservative predicate: `false` guarantees the adjusted current node is HTML,
    /// so no token can require foreign-content handling.
    pub fn may_have_foreign_content(&self) -> bool {
        self.open_foreign_count > 0 || self.fragment_context_node.is_some()
    }

    pub fn should_dispatch_to_foreign(&self, token: &Token) -> bool {
        if self.open_elements.is_empty() {
            return false;
        }
        // Fast path for HTML-only documents (the common case): no foreign element is open,
        // so the adjusted current node cannot be in the SVG/MathML namespaces.
        if self.open_foreign_count == 0 && self.fragment_context_node.is_none() {
            return false;
        }
        let Some(adjusted_node) = self.adjusted_current_node() else {
            return false;
        };
        let Some(el) = self.doc.node(adjusted_node).as_element() else {
            return false;
        };

        if el.namespace == Namespace::Html {
            return false;
        }

        if matches!(token, Token::Eof) {
            return false;
        }

        if self.is_mathml_text_integration_point(adjusted_node) {
            match token {
                Token::StartTag { name, .. } if name != "mglyph" && name != "malignmark" => {
                    return false;
                }
                Token::Character(_) => {
                    return false;
                }
                _ => {}
            }
        }

        if el.namespace == Namespace::MathMl
            && el.local_name == "annotation-xml"
            && let Token::StartTag { name, .. } = token
            && name == "svg"
        {
            return false;
        }

        if self.is_html_integration_point(adjusted_node) {
            match token {
                Token::StartTag { .. } | Token::Character(_) => {
                    return false;
                }
                _ => {}
            }
        }

        true
    }

    /// Checks whether an HTML element with `target_name` is in the specific scope defined
    /// by `scope_list` and optional `extra_html_tags` (§13.2.4.2).
    pub fn has_element_in_specific_scope(
        &self,
        target_name: &str,
        scope_list: &[(&str, Namespace)],
        extra_html_tags: &[&str],
    ) -> bool {
        // Anti-quadratic fast path (audit finding F2): if no element with this tag name is
        // open at all, no scan can find one. This is the common case for `close_p_element`
        // checks, `</p>`, `</div>`-style end tags, etc.; it keeps per-token cost O(1)
        // instead of O(stack depth).
        if self.open_html_count(target_name) == 0 {
            return false;
        }
        for &node_id in self.open_elements.iter().rev() {
            let Some(el) = self.doc.node(node_id).as_element() else {
                continue;
            };
            if el.namespace == Namespace::Html && el.local_name == target_name {
                return true;
            }
            if scope_list
                .iter()
                .any(|&(s_name, s_ns)| el.namespace == s_ns && el.local_name == s_name)
            {
                return false;
            }
            if el.namespace == Namespace::Html && extra_html_tags.contains(&el.local_name.as_str())
            {
                return false;
            }
        }
        false
    }

    pub fn has_element_in_scope(&self, target_name: &str) -> bool {
        self.has_element_in_specific_scope(target_name, DEFAULT_SCOPE_ELEMENTS, &[])
    }

    pub fn has_node_in_scope(&self, target_node: NodeId) -> bool {
        // Fast path: a node that is not on the stack cannot be in scope.
        if !self.open_ids.contains(&target_node) {
            return false;
        }
        for &node_id in self.open_elements.iter().rev() {
            if node_id == target_node {
                return true;
            }
            if let Some(el) = self.doc.node(node_id).as_element()
                && DEFAULT_SCOPE_ELEMENTS
                    .iter()
                    .any(|&(s_name, s_ns)| el.namespace == s_ns && el.local_name == s_name)
            {
                return false;
            }
        }
        false
    }

    pub fn has_element_in_button_scope(&self, target_name: &str) -> bool {
        self.has_element_in_specific_scope(target_name, DEFAULT_SCOPE_ELEMENTS, &["button"])
    }

    pub fn has_element_in_list_item_scope(&self, target_name: &str) -> bool {
        self.has_element_in_specific_scope(target_name, DEFAULT_SCOPE_ELEMENTS, &["ol", "ul"])
    }

    pub fn has_element_in_table_scope(&self, target_name: &str) -> bool {
        const TABLE_SCOPE: &[(&str, Namespace)] = &[
            ("html", Namespace::Html),
            ("table", Namespace::Html),
            ("template", Namespace::Html),
        ];
        self.has_element_in_specific_scope(target_name, TABLE_SCOPE, &[])
    }

    pub fn has_table_body_in_table_scope(&self) -> bool {
        self.has_element_in_table_scope("tbody")
            || self.has_element_in_table_scope("thead")
            || self.has_element_in_table_scope("tfoot")
    }

    pub fn generate_implied_end_tags(&mut self, except: Option<&str>) {
        while let Some(curr) = self.current_node() {
            let should_pop = self.doc.node(curr).as_element().is_some_and(|el| {
                el.namespace == Namespace::Html
                    && IMPLIED_END_TAG_ELEMENTS.contains(&el.local_name.as_str())
                    && Some(el.local_name.as_str()) != except
            });
            if should_pop {
                self.pop_open_element();
            } else {
                break;
            }
        }
    }

    pub fn close_p_element(&mut self) {
        self.generate_implied_end_tags(Some("p"));
        while let Some(node_id) = self.pop_open_element() {
            if self.node_in_html(node_id, "p") {
                break;
            }
        }
    }

    /// Computes the "appropriate place for inserting a node" (§13.2.6.1), including
    /// foster parenting when `self.foster_parenting` is active and the target is a table element.
    fn appropriate_place_for_inserting_node(
        &self,
        override_target: Option<NodeId>,
    ) -> AdjustedInsertionLocation {
        let target = override_target
            .or_else(|| self.current_node())
            .unwrap_or_else(|| self.doc.root());

        let is_foster_target = self.foster_parenting
            && self.doc.node(target).as_element().is_some_and(|el| {
                el.namespace == Namespace::Html
                    && matches!(
                        el.local_name.as_str(),
                        "table" | "tbody" | "tfoot" | "thead" | "tr"
                    )
            });

        let mut loc = if is_foster_target {
            let last_template = self
                .open_elements
                .iter()
                .rposition(|&id| self.node_in_html(id, "template"));
            let last_table = self
                .open_elements
                .iter()
                .rposition(|&id| self.node_in_html(id, "table"));

            if let Some(tmpl_idx) = last_template
                && last_table.is_none_or(|tbl_idx| tmpl_idx > tbl_idx)
            {
                let tmpl_id = self.open_elements[tmpl_idx];
                let frag_id = self
                    .doc
                    .node(tmpl_id)
                    .as_element()
                    .and_then(|el| el.template_contents)
                    .unwrap_or(tmpl_id);
                return AdjustedInsertionLocation {
                    parent: frag_id,
                    insert_before_sibling: None,
                };
            }

            if let Some(tbl_idx) = last_table {
                let table_id = self.open_elements[tbl_idx];
                if let Some(parent_id) = self.doc.node(table_id).parent {
                    AdjustedInsertionLocation {
                        parent: parent_id,
                        insert_before_sibling: Some(table_id),
                    }
                } else {
                    let prev_el = self.open_elements[tbl_idx.saturating_sub(1)];
                    AdjustedInsertionLocation {
                        parent: prev_el,
                        insert_before_sibling: None,
                    }
                }
            } else {
                AdjustedInsertionLocation {
                    parent: self
                        .open_elements
                        .first()
                        .copied()
                        .unwrap_or(self.doc.root()),
                    insert_before_sibling: None,
                }
            }
        } else {
            AdjustedInsertionLocation {
                parent: target,
                insert_before_sibling: None,
            }
        };

        // If adjusted insertion location is inside a `<template>` element, redirect to its template_contents
        if let Some(el) = self.doc.node(loc.parent).as_element()
            && el.namespace == Namespace::Html
            && el.local_name == "template"
            && let Some(frag_id) = el.template_contents
        {
            loc.parent = frag_id;
            loc.insert_before_sibling = None;
        }
        loc
    }

    /// Returns the current node when it is the HTML `<script>` element whose text content
    /// has just been tokenized by the "text" insertion mode.
    fn current_script_element(&self) -> Option<NodeId> {
        let current = self.current_node()?;
        self.doc
            .node(current)
            .as_element()
            .filter(|element| {
                element.namespace == Namespace::Html && element.local_name == "script"
            })
            .map(|_| current)
    }

    /// WP-6.4: executes a parser-inserted `<script>` element (inline scripts only).
    ///
    /// Called from the "text" insertion mode once the script's end tag (or EOF) has been
    /// seen, i.e. after the tokenizer stored the script data as the element's text child and
    /// before the parser continues with the following input. The executor may mutate the DOM
    /// and write HTML with `document.write`; written HTML is queued and inserted into the
    /// tokenizer at the current position by [`drive_tokenizer`], so it is re-tokenized before
    /// the rest of the input. Execution errors are non-fatal and collected in
    /// [`ParseOutcome::script_errors`].
    ///
    /// External scripts (`src`) are skipped: fetching external resources belongs to the host
    /// (the render pipeline runs them post-parse), so the parser hook does not attempt it.
    fn execute_parser_inserted_script(&mut self, script_id: NodeId) {
        if !self.scripting_enabled || self.script_executor.is_none() {
            return;
        }
        let has_src = self
            .doc
            .node(script_id)
            .as_element()
            .is_some_and(|element| element.get_attribute("src").is_some());
        if has_src {
            return;
        }
        // §13.2.6.4.7 inserted the script data as text children of the script element.
        let mut script_text = String::new();
        let mut child = self.doc.node(script_id).first_child;
        while let Some(child_id) = child {
            if let Some(data) = self.doc.node(child_id).as_text() {
                script_text.push_str(data);
            }
            child = self.doc.node(child_id).next_sibling;
        }
        let Self {
            doc,
            script_executor,
            pending_written_html,
            script_errors,
            ..
        } = self;
        let Some(executor) = script_executor.as_mut() else {
            return;
        };
        let ScriptExecution {
            written_html,
            error,
        } = executor.execute(doc, &script_text, "<script>");
        if !written_html.is_empty() {
            pending_written_html.push_str(&written_html);
        }
        if let Some(error) = error {
            script_errors.push(error);
        }
    }

    fn insert_node_at_adjusted_location(&mut self, node_id: NodeId) {
        let loc = self.appropriate_place_for_inserting_node(None);
        // §13.2.6 inserts at the adjusted location, but once a script has detached the
        // document element the location can be the Document node itself. Browsers drop such
        // tokens instead of creating a second document element (`scripted_foster01`); this
        // mirrors the pre-existing guard in `insert_character`/`insert_text_run`.
        if loc.parent == self.doc.root() && self.html_element_seen {
            return;
        }
        if let Some(sib) = loc.insert_before_sibling {
            self.doc.insert_before(sib, node_id);
        } else {
            self.doc.append_child(loc.parent, node_id);
        }
    }

    /// Converts raw `(name, value)` attribute pairs into [`Attribute`]s, enforcing
    /// [`ParseOptions::max_attributes_per_element`] and counting dropped attributes.
    fn build_attributes(&mut self, raw_attrs: &[(String, String)]) -> Vec<Attribute> {
        let kept = self.cap_attribute_slice(raw_attrs).len();
        raw_attrs[..kept]
            .iter()
            .map(|(k, v)| Attribute::new(k.clone(), v.clone()))
            .collect()
    }

    /// Truncates `raw_attrs` to the configured per-element limit, counting violations.
    fn cap_attribute_slice<'a>(
        &mut self,
        raw_attrs: &'a [(String, String)],
    ) -> &'a [(String, String)] {
        let max = self.parse_options.max_attributes_per_element;
        if raw_attrs.len() > max {
            self.limit_violations.attributes_dropped += (raw_attrs.len() - max) as u64;
            &raw_attrs[..max]
        } else {
            raw_attrs
        }
    }

    fn insert_element(
        &mut self,
        namespace: Namespace,
        name: &str,
        raw_attrs: &[(String, String)],
    ) -> NodeId {
        if namespace == Namespace::Html && name == "select" {
            self.has_select_element = true;
        }
        let attrs = self.build_attributes(raw_attrs);
        let el_id = self.doc.create_element(namespace, name, attrs);
        self.insert_node_at_adjusted_location(el_id);
        self.push_open_element(el_id);
        el_id
    }

    fn insert_html_element(&mut self, name: &str, raw_attrs: &[(String, String)]) -> NodeId {
        self.insert_element(Namespace::Html, name, raw_attrs)
    }

    fn create_html_element(&mut self, name: &str, raw_attrs: &[(String, String)]) -> NodeId {
        let attrs = self.build_attributes(raw_attrs);
        self.doc.create_element(Namespace::Html, name, attrs)
    }

    fn create_foreign_element(
        &mut self,
        ns: Namespace,
        mut name: String,
        raw_attrs: &[(String, String)],
    ) -> NodeId {
        if ns == Namespace::Svg {
            let name_lower = name.to_ascii_lowercase();
            if let Some(&(_, adj_name)) = SVG_ELEMENT_NAME_ADJUSTMENTS
                .iter()
                .find(|&&(orig, _)| orig == name_lower)
            {
                name = adj_name.to_owned();
            }
        }

        let raw_attrs = self.cap_attribute_slice(raw_attrs);
        let mut attrs: Vec<Attribute> = Vec::with_capacity(raw_attrs.len());
        for (raw_k, v) in raw_attrs {
            let mut k = raw_k.clone();
            if ns == Namespace::MathMl && k == "definitionurl" {
                k = "definitionURL".to_owned();
            }
            if ns == Namespace::Svg {
                let k_lower = k.to_ascii_lowercase();
                if let Some(&(_, adj_attr)) = SVG_ATTRIBUTE_ADJUSTMENTS
                    .iter()
                    .find(|&&(orig, _)| orig == k_lower)
                {
                    k = adj_attr.to_owned();
                }
            }

            if let Some(&(_, prefix, local, attr_ns)) = FOREIGN_ATTRIBUTE_ADJUSTMENTS
                .iter()
                .find(|&&(orig, _, _, _)| orig == k)
            {
                attrs.push(Attribute {
                    namespace: attr_ns,
                    prefix: prefix.map(str::to_owned),
                    local_name: local.to_owned(),
                    value: v.clone(),
                });
            } else {
                attrs.push(Attribute::new(k, v.clone()));
            }
        }

        self.doc.create_element(ns, name, attrs)
    }

    fn insert_foreign_element(
        &mut self,
        ns: Namespace,
        name: String,
        raw_attrs: &[(String, String)],
    ) -> NodeId {
        let el_id = self.create_foreign_element(ns, name, raw_attrs);
        self.insert_node_at_adjusted_location(el_id);
        self.push_open_element(el_id);
        el_id
    }

    fn insert_comment(&mut self, data: &str, override_parent: Option<NodeId>) {
        let comment_id = self.doc.create_node(NodeKind::Comment(data.to_owned()));
        if let Some(parent_id) = override_parent {
            self.doc.append_child(parent_id, comment_id);
        } else {
            self.insert_node_at_adjusted_location(comment_id);
        }
    }

    fn insert_processing_instruction(&mut self, target: &str, data: &str) {
        let pi_id = self.doc.create_node(NodeKind::ProcessingInstruction {
            target: target.to_owned(),
            data: data.to_owned(),
        });
        self.insert_node_at_adjusted_location(pi_id);
    }

    fn insert_character(&mut self, cp: u32) {
        let Some(ch) = char::from_u32(cp) else {
            return;
        };
        let mut buf = [0u8; 4];
        let s = ch.encode_utf8(&mut buf);
        // Fast path: outside foster-parenting the appropriate insertion location is simply
        // the current node, unless it is an HTML `<template>` (whose characters are
        // redirected into its inert contents fragment). Avoids the full location
        // computation (several arena lookups) for every text character.
        if !self.foster_parenting
            && let Some(current) = self.current_node()
            && let Some(element) = self.doc.node(current).as_element()
            && !(element.namespace == Namespace::Html
                && element.local_name == "template"
                && element.template_contents.is_some())
        {
            self.doc.append_text(current, s);
            return;
        }
        let loc = self.appropriate_place_for_inserting_node(None);
        if loc.parent == self.doc.root() {
            return;
        }
        if let Some(sib) = loc.insert_before_sibling {
            self.doc.insert_text_before(sib, s);
        } else {
            self.doc.append_text(loc.parent, s);
        }
    }

    /// Processes a run of consecutive character tokens in the "in body" insertion mode.
    ///
    /// The caller guarantees that `self.mode == InsertionMode::InBody` and that no other
    /// token intervenes, so the observable behaviour is identical to calling
    /// [`TreeBuilder::process_token`] for each code point (filters NUL/invalid code points,
    /// reconstructs the active formatting elements, sets `frameset_ok` for non-whitespace
    /// and inserts the coalesced text): it just computes the insertion location once for
    /// the whole run and appends a single `&str` instead of one `char` at a time.
    pub fn process_character_run(&mut self, bytes: &[u8]) {
        // The driver batches consecutive character code points into UTF-8 bytes (the
        // tokenizer's data-state fast path writes ASCII bytes directly). Validating the
        // whole run once and classifying whitespace at byte level avoids a
        // `char::from_u32` + `String::push` per code point for the (dominant) text path.
        let mut bytes = bytes;
        if self.skip_next_lf {
            // `process_token` clears the flag for the first token; only an immediate LF is
            // swallowed (e.g. the newline right after <pre>/<textarea>).
            self.skip_next_lf = false;
            if bytes.first() == Some(&b'\n') {
                bytes = &bytes[1..];
            }
        }
        if bytes.is_empty() {
            return;
        }
        let Ok(text) = std::str::from_utf8(bytes) else {
            return;
        };
        let has_non_whitespace = bytes
            .iter()
            .any(|byte| !matches!(byte, b'\t' | b'\n' | 0x0C | b'\r' | b' '));
        self.reconstruct_active_formatting_elements();
        if has_non_whitespace && self.template_modes.is_empty() {
            self.frameset_ok = false;
        }
        self.insert_text_run(text);
    }

    /// Inserts `text` at the appropriate insertion location (the multi-character version
    /// of [`TreeBuilder::insert_character`]).
    fn insert_text_run(&mut self, text: &str) {
        if !self.foster_parenting
            && let Some(current) = self.current_node()
            && let Some(element) = self.doc.node(current).as_element()
            && !(element.namespace == Namespace::Html
                && element.local_name == "template"
                && element.template_contents.is_some())
        {
            self.doc.append_text(current, text);
            return;
        }
        let loc = self.appropriate_place_for_inserting_node(None);
        if loc.parent == self.doc.root() {
            return;
        }
        if let Some(sib) = loc.insert_before_sibling {
            self.doc.insert_text_before(sib, text);
        } else {
            self.doc.append_text(loc.parent, text);
        }
    }

    fn reconstruct_active_formatting_elements(&mut self) {
        if self.active_formatting.is_empty() {
            return;
        }
        match self.active_formatting.last() {
            None | Some(FormattingEntry::Marker) => return,
            Some(FormattingEntry::Element { node_id, .. }) => {
                if self.is_open_element(*node_id) {
                    return;
                }
            }
        }

        let mut i = self.active_formatting.len() - 1;
        while i > 0 {
            i -= 1;
            match &self.active_formatting[i] {
                FormattingEntry::Marker => {
                    i += 1;
                    break;
                }
                FormattingEntry::Element { node_id, .. } => {
                    if self.is_open_element(*node_id) {
                        i += 1;
                        break;
                    }
                }
            }
        }

        while i < self.active_formatting.len() {
            if let FormattingEntry::Element {
                tag_name,
                attributes,
                ..
            } = self.active_formatting[i].clone()
            {
                let new_id = self.insert_html_element(&tag_name, &attributes);
                self.active_formatting[i] = FormattingEntry::Element {
                    node_id: new_id,
                    tag_name,
                    attributes,
                };
            }
            i += 1;
        }
    }

    fn push_active_formatting(&mut self, node_id: NodeId, name: &str, attrs: &[(String, String)]) {
        // Noah's Ark clause: at most 3 identical elements after the last marker. Only the
        // earliest of the matches is removed when a fourth appears, so a forward scan that
        // keeps the first match and stops at the third is equivalent to the old reverse
        // scan that collected every match (audit finding F2: that scan was O(active list)
        // per formatting element). The common cases (fewer than three entries after the
        // last marker, or no identical triple) no longer allocate or scan at all.
        let region_start = self
            .active_formatting
            .iter()
            .rposition(|entry| matches!(entry, FormattingEntry::Marker))
            .map_or(0, |marker_idx| marker_idx + 1);
        let region_len = self.active_formatting.len() - region_start;
        if region_len >= 3 {
            let mut first_match_idx = None;
            let mut matches = 0usize;
            for (idx, entry) in self.active_formatting.iter().enumerate().skip(region_start) {
                if let FormattingEntry::Element {
                    tag_name,
                    attributes,
                    ..
                } = entry
                    && tag_name == name
                    && attributes == attrs
                {
                    matches += 1;
                    if first_match_idx.is_none() {
                        first_match_idx = Some(idx);
                    }
                    if matches == 3 {
                        if let Some(earliest_idx) = first_match_idx {
                            self.active_formatting.remove(earliest_idx);
                        }
                        break;
                    }
                }
            }
        }
        self.active_formatting.push(FormattingEntry::Element {
            node_id,
            tag_name: name.to_owned(),
            attributes: attrs.to_vec(),
        });
    }

    fn clear_active_formatting_to_last_marker(&mut self) {
        while let Some(entry) = self.active_formatting.pop() {
            if entry == FormattingEntry::Marker {
                break;
            }
        }
    }

    fn process_any_other_end_tag(&mut self, name: &str) {
        // Fast path (anti-quadratic F2): when no matching element is open, the walk below
        // always falls through without acting; skip the O(stack) scan entirely.
        if self.open_html_count(name) == 0 {
            return;
        }
        for idx in (0..self.open_elements.len()).rev() {
            let node_id = self.open_elements[idx];
            if self.node_in_html(node_id, name) {
                self.generate_implied_end_tags(Some(name));
                while self.open_elements.len() > idx {
                    self.pop_open_element();
                }
                break;
            }
            if self.is_node_special(node_id) {
                break;
            }
        }
    }

    fn run_adoption_agency_algorithm(&mut self, subject: &str) {
        if let Some(curr) = self.current_node()
            && self.node_in_html(curr, subject)
        {
            let in_active = self
                .active_formatting
                .iter()
                .any(|e| matches!(e, FormattingEntry::Element { node_id, .. } if *node_id == curr));
            if !in_active {
                self.pop_open_element();
                return;
            }
        }

        let mut outer_loop_counter = 0;

        loop {
            if outer_loop_counter >= 8 {
                return;
            }
            outer_loop_counter += 1;

            let mut formatting_idx = None;
            for (idx, entry) in self.active_formatting.iter().enumerate().rev() {
                match entry {
                    FormattingEntry::Marker => break,
                    FormattingEntry::Element { tag_name, .. } if tag_name == subject => {
                        formatting_idx = Some(idx);
                        break;
                    }
                    _ => {}
                }
            }

            let Some(fmt_idx) = formatting_idx else {
                self.process_any_other_end_tag(subject);
                return;
            };

            let (fmt_node_id, fmt_name, fmt_attrs) = match &self.active_formatting[fmt_idx] {
                FormattingEntry::Element {
                    node_id,
                    tag_name,
                    attributes,
                } => (*node_id, tag_name.clone(), attributes.clone()),
                FormattingEntry::Marker => unreachable!(),
            };

            let Some(fmt_stack_pos) = self.open_elements.iter().rposition(|&id| id == fmt_node_id)
            else {
                self.active_formatting.remove(fmt_idx);
                return;
            };

            if !self.has_element_in_scope(&fmt_name) {
                return;
            }

            let furthest_block_idx = self.open_elements[fmt_stack_pos + 1..]
                .iter()
                .position(|&id| self.is_node_special(id))
                .map(|p| fmt_stack_pos + 1 + p);

            let Some(furthest_block_pos) = furthest_block_idx else {
                while let Some(popped) = self.pop_open_element() {
                    if popped == fmt_node_id {
                        break;
                    }
                }
                if let Some(pos) = self.active_formatting.iter().rposition(|e| {
                    matches!(e, FormattingEntry::Element { node_id, .. } if *node_id == fmt_node_id)
                }) {
                    self.active_formatting.remove(pos);
                }
                return;
            };

            let furthest_block_id = self.open_elements[furthest_block_pos];
            // `fmt_stack_pos == 0` cannot happen for trees built by the parser (the
            // formatted element is never the bottom-most open element), but a manually
            // built `TreeBuilder`/corrupted stack would underflow here (the old
            // `fmt_stack_pos - 1` panicked or wrapped in release). Bail out cleanly.
            let Some(common_ancestor_id) = fmt_stack_pos
                .checked_sub(1)
                .and_then(|pos| self.open_elements.get(pos).copied())
            else {
                return;
            };
            let mut bookmark = fmt_idx;
            let mut node = furthest_block_id;
            let mut last_node = furthest_block_id;
            let mut inner_loop_counter = 0;
            let mut bookmark_moved = false;
            let mut next_node_override = None;

            loop {
                inner_loop_counter += 1;

                let above_node = if let Some(n) = next_node_override.take() {
                    n
                } else {
                    let Some(pos) = self.open_elements.iter().rposition(|&id| id == node) else {
                        break;
                    };
                    if pos == 0 {
                        break;
                    }
                    self.open_elements[pos - 1]
                };
                node = above_node;

                if node == fmt_node_id {
                    break;
                }

                let node_fmt_idx = self.active_formatting.iter().rposition(
                    |e| matches!(e, FormattingEntry::Element { node_id, .. } if *node_id == node),
                );
                if inner_loop_counter > 3
                    && let Some(idx) = node_fmt_idx
                {
                    self.active_formatting.remove(idx);
                    if idx <= bookmark && bookmark > 0 {
                        bookmark -= 1;
                    }
                }

                let Some(fmt_entry_pos) = self.active_formatting.iter().rposition(
                    |e| matches!(e, FormattingEntry::Element { node_id, .. } if *node_id == node),
                ) else {
                    if let Some(pos) = self.open_elements.iter().rposition(|&id| id == node) {
                        next_node_override = if pos > 0 {
                            Some(self.open_elements[pos - 1])
                        } else {
                            None
                        };
                        self.remove_open_element(pos);
                    }
                    continue;
                };

                let (n_name, n_attrs) = match &self.active_formatting[fmt_entry_pos] {
                    FormattingEntry::Element {
                        tag_name,
                        attributes,
                        ..
                    } => (tag_name.clone(), attributes.clone()),
                    _ => unreachable!(),
                };
                let new_node = self.create_html_element(&n_name, &n_attrs);
                self.active_formatting[fmt_entry_pos] = FormattingEntry::Element {
                    node_id: new_node,
                    tag_name: n_name,
                    attributes: n_attrs,
                };
                let stack_pos = self
                    .open_elements
                    .iter()
                    .rposition(|&id| id == node)
                    .unwrap();
                self.replace_open_element(stack_pos, new_node);
                node = new_node;

                if !bookmark_moved {
                    bookmark = fmt_entry_pos + 1;
                    bookmark_moved = true;
                }

                self.doc.detach(last_node);
                self.doc.append_child(node, last_node);
                last_node = node;
            }

            let loc = self.appropriate_place_for_inserting_node(Some(common_ancestor_id));
            self.doc.detach(last_node);
            if let Some(before) = loc.insert_before_sibling {
                self.doc.insert_before(before, last_node);
            } else {
                self.doc.append_child(loc.parent, last_node);
            }

            let new_element = self.create_html_element(&fmt_name, &fmt_attrs);
            self.doc.reparent_children(furthest_block_id, new_element);
            self.doc.append_child(furthest_block_id, new_element);

            if let Some(old_pos) = self.active_formatting.iter().rposition(|e| {
                matches!(e, FormattingEntry::Element { node_id, .. } if *node_id == fmt_node_id)
            }) {
                self.active_formatting.remove(old_pos);
                if old_pos < bookmark && bookmark > 0 {
                    bookmark -= 1;
                }
            }
            let insert_bookmark = bookmark.min(self.active_formatting.len());
            self.active_formatting.insert(
                insert_bookmark,
                FormattingEntry::Element {
                    node_id: new_element,
                    tag_name: fmt_name,
                    attributes: fmt_attrs,
                },
            );

            if let Some(old_stack_pos) =
                self.open_elements.iter().rposition(|&id| id == fmt_node_id)
            {
                self.remove_open_element(old_stack_pos);
            }
            let fb_stack_pos = self
                .open_elements
                .iter()
                .rposition(|&id| id == furthest_block_id)
                .expect("furthest_block must be in open_elements");
            self.insert_open_element(fb_stack_pos + 1, new_element);
        }
    }

    fn clear_stack_back_to_table_context(&mut self) {
        while let Some(curr) = self.current_node() {
            if self.node_in_html(curr, "table")
                || self.node_in_html(curr, "template")
                || self.node_in_html(curr, "html")
            {
                break;
            }
            self.pop_open_element();
        }
    }

    fn clear_stack_back_to_table_body_context(&mut self) {
        while let Some(curr) = self.current_node() {
            if self.node_in_html(curr, "tbody")
                || self.node_in_html(curr, "tfoot")
                || self.node_in_html(curr, "thead")
                || self.node_in_html(curr, "template")
                || self.node_in_html(curr, "html")
            {
                break;
            }
            self.pop_open_element();
        }
    }

    fn clear_stack_back_to_table_row_context(&mut self) {
        while let Some(curr) = self.current_node() {
            if self.node_in_html(curr, "tr")
                || self.node_in_html(curr, "template")
                || self.node_in_html(curr, "html")
            {
                break;
            }
            self.pop_open_element();
        }
    }

    pub fn reset_insertion_mode_appropriately(&mut self) {
        for (idx, &node_id) in self.open_elements.iter().enumerate().rev() {
            let last = idx == 0;
            let (ns, name) = if last && let Some((ref f_ns, ref f_name)) = self.fragment_context {
                (*f_ns, f_name.as_str())
            } else if let Some(el) = self.doc.node(node_id).as_element() {
                (el.namespace, el.local_name.as_str())
            } else {
                continue;
            };

            if ns != Namespace::Html {
                if last {
                    self.mode = InsertionMode::InBody;
                    return;
                }
                continue;
            }

            match name {
                "td" | "th" if !last => {
                    self.mode = InsertionMode::InCell;
                    return;
                }
                "tr" => {
                    self.mode = InsertionMode::InRow;
                    return;
                }
                "tbody" | "thead" | "tfoot" => {
                    self.mode = InsertionMode::InTableBody;
                    return;
                }
                "caption" => {
                    self.mode = InsertionMode::InCaption;
                    return;
                }
                "colgroup" => {
                    self.mode = InsertionMode::InColumnGroup;
                    return;
                }
                "table" => {
                    self.mode = InsertionMode::InTable;
                    return;
                }
                "template" => {
                    if let Some(&tm) = self.template_modes.last() {
                        self.mode = tm;
                        return;
                    }
                }
                "head" if !last => {
                    self.mode = InsertionMode::InHead;
                    return;
                }
                "body" => {
                    self.mode = InsertionMode::InBody;
                    return;
                }
                "frameset" => {
                    self.mode = InsertionMode::InFrameset;
                    return;
                }
                "html" => {
                    self.mode = if self.head_element.is_none() {
                        InsertionMode::BeforeHead
                    } else {
                        InsertionMode::AfterHead
                    };
                    return;
                }
                _ => {}
            }
            if last {
                self.mode = InsertionMode::InBody;
                return;
            }
        }
        self.mode = InsertionMode::InBody;
    }

    fn process_in_foreign_content(&mut self, token: Token) -> Option<TokenizerState> {
        let adjusted_node = self
            .adjusted_current_node()
            .expect("adjusted node in foreign");
        let adjusted_ns = self
            .doc
            .node(adjusted_node)
            .as_element()
            .map(|el| el.namespace)
            .unwrap_or(Namespace::Html);

        match token {
            Token::Character(0x00) => {
                self.insert_character(0xFFFD);
                None
            }
            Token::Character(cp @ (0x09 | 0x0A | 0x0C | 0x0D | 0x20)) => {
                self.insert_character(cp);
                None
            }
            Token::Character(cp) => {
                self.insert_character(cp);
                self.frameset_ok = false;
                None
            }
            Token::Comment(data) => {
                self.insert_comment(&data, None);
                None
            }
            Token::ProcessingInstruction { target, data } => {
                self.insert_processing_instruction(&target, &data);
                None
            }
            Token::Doctype { .. } => None,
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if FOREIGN_BREAKOUT_TAGS.contains(&name.as_str())
                || (name == "font"
                    && attributes
                        .iter()
                        .any(|(k, _)| matches!(k.as_str(), "color" | "face" | "size"))) =>
            {
                while let Some(curr) = self.current_node() {
                    if self.is_integration_point_or_html(curr) {
                        break;
                    }
                    self.pop_open_element();
                }
                self.process_token_html(token)
            }
            Token::EndTag { ref name, .. } if matches!(name.as_str(), "br" | "p") => {
                while let Some(curr) = self.current_node() {
                    if self.is_integration_point_or_html(curr) {
                        break;
                    }
                    self.pop_open_element();
                }
                self.process_token_html(token)
            }
            Token::StartTag {
                name,
                attributes,
                self_closing,
            } => {
                self.insert_foreign_element(adjusted_ns, name, &attributes);
                if self_closing {
                    self.pop_open_element();
                }
                None
            }
            Token::EndTag { ref name, .. }
                if name == "script"
                    && self
                        .current_node()
                        .is_some_and(|id| self.node_is(id, Namespace::Svg, "script")) =>
            {
                self.pop_open_element();
                None
            }
            Token::EndTag { ref name, .. } => {
                let name_lower = name.to_ascii_lowercase();
                let mut current_idx = self.open_elements.len().saturating_sub(1);
                loop {
                    if current_idx == 0 {
                        return None;
                    }
                    let node_id = self.open_elements[current_idx];
                    let el = self.doc.node(node_id).as_element()?;
                    if el.local_name.eq_ignore_ascii_case(&name_lower) {
                        while self.open_elements.len() > current_idx {
                            self.pop_open_element();
                        }
                        return None;
                    }
                    current_idx -= 1;
                    let prev_node_id = self.open_elements[current_idx];
                    let prev_el = self.doc.node(prev_node_id).as_element()?;
                    if prev_el.namespace != Namespace::Html {
                        continue;
                    }
                    return self.process_token_html(Token::EndTag {
                        name: name.clone(),
                        attributes: Vec::new(),
                        self_closing: false,
                    });
                }
            }
            Token::Eof => self.process_token_html(Token::Eof),
        }
    }

    /// Processes a single token and returns an optional [`TokenizerState`] switch for the tokenizer.
    pub fn process_token(&mut self, token: Token) -> Option<TokenizerState> {
        if self.skip_next_lf {
            self.skip_next_lf = false;
            if token == Token::Character(0x0A) {
                return None;
            }
        }

        // Anti-DoS: stop growing the tree once a hard limit is reached. The token is
        // ignored (never inserted) and the violation is counted; parsing continues so
        // the rest of the input is drained in linear time instead of panicking/aborting.
        match token {
            Token::StartTag { .. } => {
                if self.open_elements.len() >= self.parse_options.max_depth {
                    self.limit_violations.depth_exceeded += 1;
                    return None;
                }
                if self.doc.len() >= self.parse_options.max_nodes {
                    self.limit_violations.nodes_exceeded += 1;
                    return None;
                }
            }
            // Comments, doctypes, PIs and characters can also allocate nodes (or
            // coalesce into new ones), so the node budget gates them too.
            Token::Eof => {}
            _ => {
                if self.doc.len() >= self.parse_options.max_nodes {
                    self.limit_violations.nodes_exceeded += 1;
                    return None;
                }
            }
        }

        if self.should_dispatch_to_foreign(&token) {
            self.process_in_foreign_content(token)
        } else {
            self.process_token_html(token)
        }
    }

    fn process_token_html(&mut self, token: Token) -> Option<TokenizerState> {
        match self.mode {
            InsertionMode::Initial => match token {
                Token::Character(0x09 | 0x0A | 0x0C | 0x0D | 0x20) => None,
                Token::Comment(data) => {
                    self.insert_comment(&data, Some(self.doc.root()));
                    None
                }
                Token::ProcessingInstruction { target, data } => {
                    let pi_id = self
                        .doc
                        .create_node(NodeKind::ProcessingInstruction { target, data });
                    self.doc.append_child(self.doc.root(), pi_id);
                    None
                }
                Token::Doctype {
                    name,
                    public_id,
                    system_id,
                    force_quirks,
                } => {
                    self.doc.quirks_mode = determine_doctype_quirks_mode(
                        force_quirks,
                        name.as_deref(),
                        public_id.as_deref(),
                        system_id.as_deref(),
                    );
                    let dt_id = self.doc.create_node(NodeKind::Doctype {
                        name: name.unwrap_or_default(),
                        public_id: public_id.unwrap_or_default(),
                        system_id: system_id.unwrap_or_default(),
                    });
                    self.doc.append_child(self.doc.root(), dt_id);
                    self.mode = InsertionMode::BeforeHtml;
                    None
                }
                other => {
                    self.doc.quirks_mode = QuirksMode::Quirks;
                    self.mode = InsertionMode::BeforeHtml;
                    self.process_token(other)
                }
            },

            InsertionMode::BeforeHtml => match token {
                Token::Doctype { .. } => None,
                Token::Comment(data) => {
                    self.insert_comment(&data, Some(self.doc.root()));
                    None
                }
                Token::ProcessingInstruction { target, data } => {
                    let pi_id = self
                        .doc
                        .create_node(NodeKind::ProcessingInstruction { target, data });
                    self.doc.append_child(self.doc.root(), pi_id);
                    None
                }
                Token::Character(0x09 | 0x0A | 0x0C | 0x0D | 0x20) => None,
                Token::StartTag {
                    ref name,
                    ref attributes,
                    ..
                } if name == "html" => {
                    self.insert_html_element("html", attributes);
                    self.html_element_seen = true;
                    self.mode = InsertionMode::BeforeHead;
                    None
                }
                Token::EndTag { ref name, .. }
                    if !matches!(name.as_str(), "head" | "body" | "html" | "br") =>
                {
                    None
                }
                other => {
                    self.insert_html_element("html", &[]);
                    self.html_element_seen = true;
                    self.mode = InsertionMode::BeforeHead;
                    self.process_token(other)
                }
            },

            InsertionMode::BeforeHead => match token {
                Token::Character(0x09 | 0x0A | 0x0C | 0x0D | 0x20) => None,
                Token::Comment(data) => {
                    self.insert_comment(&data, None);
                    None
                }
                Token::ProcessingInstruction { target, data } => {
                    self.insert_processing_instruction(&target, &data);
                    None
                }
                Token::Doctype { .. } => None,
                Token::StartTag {
                    ref name,
                    ref attributes,
                    ..
                } if name == "html" => self.process_in_body(token),
                Token::StartTag {
                    ref name,
                    ref attributes,
                    ..
                } if name == "head" => {
                    let head_id = self.insert_html_element("head", attributes);
                    self.head_element = Some(head_id);
                    self.mode = InsertionMode::InHead;
                    None
                }
                Token::EndTag { ref name, .. }
                    if !matches!(name.as_str(), "head" | "body" | "html" | "br") =>
                {
                    None
                }
                other => {
                    let head_id = self.insert_html_element("head", &[]);
                    self.head_element = Some(head_id);
                    self.mode = InsertionMode::InHead;
                    self.process_token(other)
                }
            },

            InsertionMode::InHead => self.process_in_head(token),

            InsertionMode::InHeadNoscript => match token {
                Token::Doctype { .. } => None,
                Token::StartTag { ref name, .. } if name == "html" => self.process_in_body(token),
                Token::EndTag { ref name, .. } if name == "noscript" => {
                    self.pop_open_element();
                    self.mode = InsertionMode::InHead;
                    None
                }
                Token::Character(0x09 | 0x0A | 0x0C | 0x0D | 0x20) => self.process_in_head(token),
                Token::Comment(_) | Token::ProcessingInstruction { .. } => {
                    self.process_in_head(token)
                }
                Token::StartTag { ref name, .. }
                    if matches!(
                        name.as_str(),
                        "basefont" | "bgsound" | "link" | "meta" | "noframes" | "style"
                    ) =>
                {
                    self.process_in_head(token)
                }
                Token::EndTag { ref name, .. } if name == "br" => {
                    self.pop_open_element();
                    self.mode = InsertionMode::InHead;
                    self.process_token(token)
                }
                Token::StartTag { ref name, .. }
                    if matches!(name.as_str(), "head" | "noscript") =>
                {
                    None
                }
                Token::EndTag { .. } => None,
                other => {
                    self.pop_open_element();
                    self.mode = InsertionMode::InHead;
                    self.process_token(other)
                }
            },

            InsertionMode::AfterHead => match token {
                Token::Character(cp @ (0x09 | 0x0A | 0x0C | 0x0D | 0x20)) => {
                    self.insert_character(cp);
                    None
                }
                Token::Comment(data) => {
                    self.insert_comment(&data, None);
                    None
                }
                Token::ProcessingInstruction { target, data } => {
                    self.insert_processing_instruction(&target, &data);
                    None
                }
                Token::Doctype { .. } => None,
                Token::StartTag { ref name, .. } if name == "html" => self.process_in_body(token),
                Token::StartTag {
                    ref name,
                    ref attributes,
                    ..
                } if name == "body" => {
                    self.insert_html_element("body", attributes);
                    self.frameset_ok = false;
                    self.mode = InsertionMode::InBody;
                    None
                }
                Token::StartTag {
                    ref name,
                    ref attributes,
                    ..
                } if name == "frameset" => {
                    self.insert_html_element("frameset", attributes);
                    self.mode = InsertionMode::InFrameset;
                    None
                }
                Token::StartTag { ref name, .. }
                    if matches!(
                        name.as_str(),
                        "base"
                            | "basefont"
                            | "bgsound"
                            | "link"
                            | "meta"
                            | "noframes"
                            | "script"
                            | "style"
                            | "template"
                            | "title"
                    ) =>
                {
                    if let Some(head_id) = self.head_element {
                        self.push_open_element(head_id);
                        let res = self.process_in_head(token);
                        if let Some(pos) = self.open_elements.iter().rposition(|&id| id == head_id)
                        {
                            self.remove_open_element(pos);
                        }
                        res
                    } else {
                        None
                    }
                }
                Token::EndTag { ref name, .. } if name == "template" => self.process_in_head(token),
                Token::EndTag { ref name, .. }
                    if !matches!(name.as_str(), "body" | "html" | "br") =>
                {
                    None
                }
                Token::StartTag { ref name, .. } if name == "head" => None,
                other => {
                    self.insert_html_element("body", &[]);
                    self.mode = InsertionMode::InBody;
                    self.process_token(other)
                }
            },

            InsertionMode::InBody => self.process_in_body(token),

            InsertionMode::Text => match token {
                Token::Character(cp) => {
                    self.insert_character(cp);
                    None
                }
                // WP-6.4 / §13.2.6.4.8 "the text insertion mode": a parser-inserted
                // `<script>` executes when its end tag (or EOF) is reached, with the
                // partially-built tree visible; `document.write` output is re-tokenized
                // immediately after this token by `drive_tokenizer`.
                Token::Eof => {
                    let script = self.current_script_element();
                    self.pop_open_element();
                    self.mode = self.original_mode;
                    if let Some(script_id) = script {
                        self.execute_parser_inserted_script(script_id);
                    }
                    self.process_token(Token::Eof)
                }
                Token::EndTag { ref name, .. } => {
                    let script = if name == "script" {
                        self.current_script_element()
                    } else {
                        None
                    };
                    self.pop_open_element();
                    self.mode = self.original_mode;
                    if let Some(script_id) = script {
                        self.execute_parser_inserted_script(script_id);
                    }
                    None
                }
                _ => None,
            },

            InsertionMode::InTable => self.process_in_table(token),
            InsertionMode::InTableText => self.process_in_table_text(token),
            InsertionMode::InCaption => self.process_in_caption(token),
            InsertionMode::InColumnGroup => self.process_in_column_group(token),
            InsertionMode::InTableBody => self.process_in_table_body(token),
            InsertionMode::InRow => self.process_in_row(token),
            InsertionMode::InCell => self.process_in_cell(token),

            InsertionMode::InTemplate => match token {
                Token::Character(_) | Token::Comment(_) | Token::Doctype { .. } => {
                    self.process_in_body(token)
                }
                Token::StartTag { ref name, .. }
                    if matches!(
                        name.as_str(),
                        "base"
                            | "basefont"
                            | "bgsound"
                            | "link"
                            | "meta"
                            | "noframes"
                            | "script"
                            | "style"
                            | "template"
                            | "title"
                    ) =>
                {
                    self.process_in_head(token)
                }
                Token::EndTag { ref name, .. } if name == "template" => self.process_in_head(token),
                Token::StartTag { ref name, .. }
                    if matches!(
                        name.as_str(),
                        "caption" | "colgroup" | "tbody" | "tfoot" | "thead"
                    ) =>
                {
                    self.template_modes.pop();
                    self.template_modes.push(InsertionMode::InTable);
                    self.mode = InsertionMode::InTable;
                    self.process_token(token)
                }
                Token::StartTag { ref name, .. } if name == "col" => {
                    self.template_modes.pop();
                    self.template_modes.push(InsertionMode::InColumnGroup);
                    self.mode = InsertionMode::InColumnGroup;
                    self.process_token(token)
                }
                Token::StartTag { ref name, .. } if name == "tr" => {
                    self.template_modes.pop();
                    self.template_modes.push(InsertionMode::InTableBody);
                    self.mode = InsertionMode::InTableBody;
                    self.process_token(token)
                }
                Token::StartTag { ref name, .. } if matches!(name.as_str(), "td" | "th") => {
                    self.template_modes.pop();
                    self.template_modes.push(InsertionMode::InRow);
                    self.mode = InsertionMode::InRow;
                    self.process_token(token)
                }
                Token::StartTag { .. } => {
                    self.template_modes.pop();
                    self.template_modes.push(InsertionMode::InBody);
                    self.mode = InsertionMode::InBody;
                    self.process_token(token)
                }
                Token::EndTag { .. } => None,
                Token::Eof => {
                    if !self.has_open_html_element("template") {
                        return None;
                    }
                    while let Some(id) = self.pop_open_element() {
                        if self.node_in_html(id, "template") {
                            break;
                        }
                    }
                    self.template_modes.pop();
                    self.reset_insertion_mode_appropriately();
                    self.process_token(Token::Eof)
                }
                Token::ProcessingInstruction { target, data } => {
                    self.insert_processing_instruction(&target, &data);
                    None
                }
            },

            InsertionMode::AfterBody => match token {
                Token::Character(0x09 | 0x0A | 0x0C | 0x0D | 0x20) => self.process_in_body(token),
                Token::Comment(data) => {
                    let html_id = self.open_elements.first().copied();
                    self.insert_comment(&data, html_id);
                    None
                }
                Token::ProcessingInstruction { target, data } => {
                    let pi_id = self
                        .doc
                        .create_node(NodeKind::ProcessingInstruction { target, data });
                    if let Some(&html_id) = self.open_elements.first() {
                        self.doc.append_child(html_id, pi_id);
                    } else {
                        self.doc.append_child(self.doc.root(), pi_id);
                    }
                    None
                }
                Token::Doctype { .. } => None,
                Token::StartTag { ref name, .. } if name == "html" => self.process_in_body(token),
                Token::EndTag { ref name, .. } if name == "html" => {
                    self.mode = InsertionMode::AfterAfterBody;
                    None
                }
                Token::Eof => None,
                other => {
                    self.mode = InsertionMode::InBody;
                    self.process_token(other)
                }
            },

            InsertionMode::InFrameset => match token {
                Token::Character(cp @ (0x09 | 0x0A | 0x0C | 0x0D | 0x20)) => {
                    self.insert_character(cp);
                    None
                }
                Token::Comment(data) => {
                    self.insert_comment(&data, None);
                    None
                }
                Token::ProcessingInstruction { target, data } => {
                    self.insert_processing_instruction(&target, &data);
                    None
                }
                Token::StartTag {
                    ref name,
                    ref attributes,
                    ..
                } if name == "frameset" => {
                    self.insert_html_element("frameset", attributes);
                    None
                }
                Token::EndTag { ref name, .. } if name == "frameset" => {
                    if self.open_elements.len() <= 1 {
                        return None;
                    }
                    self.pop_open_element();
                    if self.fragment_context.is_none()
                        && !self
                            .current_node()
                            .is_some_and(|id| self.node_in_html(id, "frameset"))
                    {
                        self.mode = InsertionMode::AfterFrameset;
                    }
                    None
                }
                Token::StartTag {
                    ref name,
                    ref attributes,
                    ..
                } if name == "frame" => {
                    self.insert_html_element("frame", attributes);
                    self.pop_open_element();
                    None
                }
                Token::StartTag { ref name, .. } if name == "noframes" => {
                    self.process_in_head(token)
                }
                _ => None,
            },

            InsertionMode::AfterFrameset => match token {
                Token::Character(cp @ (0x09 | 0x0A | 0x0C | 0x0D | 0x20)) => {
                    self.insert_character(cp);
                    None
                }
                Token::Comment(data) => {
                    self.insert_comment(&data, None);
                    None
                }
                Token::ProcessingInstruction { target, data } => {
                    self.insert_processing_instruction(&target, &data);
                    None
                }
                Token::EndTag { ref name, .. } if name == "html" => {
                    self.mode = InsertionMode::AfterAfterFrameset;
                    None
                }
                Token::StartTag { ref name, .. } if name == "noframes" => {
                    self.process_in_head(token)
                }
                _ => None,
            },

            InsertionMode::AfterAfterBody => match token {
                Token::Comment(data) => {
                    let parent = if self
                        .fragment_context
                        .as_ref()
                        .is_some_and(|(_, n)| n == "html")
                    {
                        self.open_elements
                            .first()
                            .copied()
                            .unwrap_or(self.doc.root())
                    } else {
                        self.doc.root()
                    };
                    self.insert_comment(&data, Some(parent));
                    None
                }
                Token::ProcessingInstruction { target, data } => {
                    let parent = if self
                        .fragment_context
                        .as_ref()
                        .is_some_and(|(_, n)| n == "html")
                    {
                        self.open_elements
                            .first()
                            .copied()
                            .unwrap_or(self.doc.root())
                    } else {
                        self.doc.root()
                    };
                    let pi_id = self
                        .doc
                        .create_node(NodeKind::ProcessingInstruction { target, data });
                    self.doc.append_child(parent, pi_id);
                    None
                }
                Token::Doctype { .. } => self.process_in_body(token),
                Token::Character(0x09 | 0x0A | 0x0C | 0x0D | 0x20) => self.process_in_body(token),
                Token::StartTag { ref name, .. } if name == "html" => self.process_in_body(token),
                Token::Eof => None,
                other => {
                    self.mode = InsertionMode::InBody;
                    self.process_token(other)
                }
            },

            InsertionMode::AfterAfterFrameset => match token {
                Token::Comment(data) => {
                    self.insert_comment(&data, Some(self.doc.root()));
                    None
                }
                Token::ProcessingInstruction { target, data } => {
                    let pi_id = self
                        .doc
                        .create_node(NodeKind::ProcessingInstruction { target, data });
                    self.doc.append_child(self.doc.root(), pi_id);
                    None
                }
                Token::Character(0x09 | 0x0A | 0x0C | 0x0D | 0x20) => self.process_in_body(token),
                Token::StartTag { ref name, .. } if name == "html" => self.process_in_body(token),
                Token::StartTag { ref name, .. } if name == "noframes" => {
                    self.process_in_head(token)
                }
                _ => None,
            },
        }
    }

    fn process_in_head(&mut self, token: Token) -> Option<TokenizerState> {
        match token {
            Token::Character(cp @ (0x09 | 0x0A | 0x0C | 0x0D | 0x20)) => {
                self.insert_character(cp);
                None
            }
            Token::Comment(data) => {
                self.insert_comment(&data, None);
                None
            }
            Token::ProcessingInstruction { target, data } => {
                self.insert_processing_instruction(&target, &data);
                None
            }
            Token::Doctype { .. } => None,
            Token::StartTag { ref name, .. } if name == "html" => self.process_in_body(token),
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if matches!(
                name.as_str(),
                "base" | "basefont" | "bgsound" | "link" | "meta"
            ) =>
            {
                self.insert_html_element(name, attributes);
                self.pop_open_element();
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "title" => {
                self.insert_html_element("title", attributes);
                self.original_mode = self.mode;
                self.mode = InsertionMode::Text;
                Some(TokenizerState::Rcdata)
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if (name == "noscript" && self.scripting_enabled)
                || matches!(name.as_str(), "noframes" | "style") =>
            {
                self.insert_html_element(name, attributes);
                self.original_mode = self.mode;
                self.mode = InsertionMode::Text;
                Some(TokenizerState::Rawtext)
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "noscript" && !self.scripting_enabled => {
                self.insert_html_element("noscript", attributes);
                self.mode = InsertionMode::InHeadNoscript;
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "script" => {
                self.insert_html_element("script", attributes);
                self.original_mode = self.mode;
                self.mode = InsertionMode::Text;
                Some(TokenizerState::ScriptData)
            }
            Token::EndTag { ref name, .. } if name == "head" => {
                self.pop_open_element();
                self.mode = InsertionMode::AfterHead;
                None
            }
            Token::EndTag { ref name, .. } if matches!(name.as_str(), "body" | "html" | "br") => {
                self.pop_open_element();
                self.mode = InsertionMode::AfterHead;
                self.process_token(token)
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "template" => {
                self.insert_html_element("template", attributes);
                self.active_formatting.push(FormattingEntry::Marker);
                self.mode = InsertionMode::InTemplate;
                self.template_modes.push(InsertionMode::InTemplate);
                None
            }
            Token::EndTag { ref name, .. } if name == "template" => {
                if !self.has_open_html_element("template") {
                    return None;
                }
                self.generate_implied_end_tags(None);
                while let Some(id) = self.pop_open_element() {
                    if self.node_in_html(id, "template") {
                        break;
                    }
                }
                while let Some(entry) = self.active_formatting.pop() {
                    if entry == FormattingEntry::Marker {
                        break;
                    }
                }
                self.template_modes.pop();
                self.reset_insertion_mode_appropriately();
                None
            }
            Token::StartTag { ref name, .. } if name == "head" => None,
            Token::EndTag { .. } => None,
            other => {
                self.pop_open_element();
                self.mode = InsertionMode::AfterHead;
                self.process_token(other)
            }
        }
    }

    fn process_in_body(&mut self, token: Token) -> Option<TokenizerState> {
        match token {
            Token::Character(0x00) => None,
            Token::Character(cp @ (0x09 | 0x0A | 0x0C | 0x0D | 0x20)) => {
                self.reconstruct_active_formatting_elements();
                self.insert_character(cp);
                None
            }
            Token::Character(cp) => {
                self.reconstruct_active_formatting_elements();
                self.insert_character(cp);
                if self.template_modes.is_empty() {
                    self.frameset_ok = false;
                }
                None
            }
            Token::Comment(data) => {
                self.insert_comment(&data, None);
                None
            }
            Token::ProcessingInstruction { target, data } => {
                self.insert_processing_instruction(&target, &data);
                None
            }
            Token::Doctype { .. } => None,
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "html" => {
                if !self.has_open_html_element("template")
                    && let Some(&html_id) = self.open_elements.first()
                    && let Some(el) = self.doc.node_mut(html_id).as_element_mut()
                {
                    for (k, v) in attributes {
                        el.set_attribute(Attribute::new(k.clone(), v.clone()), false);
                    }
                }
                None
            }
            Token::StartTag { ref name, .. }
                if matches!(
                    name.as_str(),
                    "base"
                        | "basefont"
                        | "bgsound"
                        | "link"
                        | "meta"
                        | "noframes"
                        | "script"
                        | "style"
                        | "title"
                ) =>
            {
                self.process_in_head(token)
            }
            Token::StartTag { ref name, .. } if name == "template" => {
                self.frameset_ok = false;
                self.process_in_head(token)
            }
            Token::EndTag { ref name, .. } if name == "template" => self.process_in_head(token),
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "body" => {
                if self.open_elements.len() >= 2 && !self.has_open_html_element("template") {
                    let body_id = self.open_elements[1];
                    if self.node_in_html(body_id, "body")
                        && let Some(el) = self.doc.node_mut(body_id).as_element_mut()
                    {
                        self.frameset_ok = false;
                        for (k, v) in attributes {
                            el.set_attribute(Attribute::new(k.clone(), v.clone()), false);
                        }
                    }
                }
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if matches!(name.as_str(), "pre" | "listing") => {
                if self.has_element_in_button_scope("p") {
                    self.close_p_element();
                }
                self.insert_html_element(name, attributes);
                self.skip_next_lf = true;
                self.frameset_ok = false;
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if matches!(name.as_str(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6") => {
                if self.has_element_in_button_scope("p") {
                    self.close_p_element();
                }
                if let Some(curr) = self.current_node()
                    && self.doc.node(curr).as_element().is_some_and(|el| {
                        el.namespace == Namespace::Html
                            && matches!(
                                el.local_name.as_str(),
                                "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
                            )
                    })
                {
                    self.pop_open_element();
                }
                self.insert_html_element(name, attributes);
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "li" => {
                self.frameset_ok = false;
                for &node_id in self.open_elements.clone().iter().rev() {
                    if self.node_in_html(node_id, "li") {
                        self.generate_implied_end_tags(Some("li"));
                        while let Some(popped) = self.pop_open_element() {
                            if popped == node_id {
                                break;
                            }
                        }
                        break;
                    }
                    if self.is_node_special(node_id)
                        && !matches!(
                            self.doc
                                .node(node_id)
                                .as_element()
                                .map(|el| (el.namespace, el.local_name.as_str())),
                            Some((Namespace::Html, "address" | "div" | "p"))
                        )
                    {
                        break;
                    }
                }
                if self.has_element_in_button_scope("p") {
                    self.close_p_element();
                }
                self.insert_html_element("li", attributes);
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if matches!(name.as_str(), "dd" | "dt") => {
                self.frameset_ok = false;
                for &node_id in self.open_elements.clone().iter().rev() {
                    let is_dd = self.node_in_html(node_id, "dd");
                    let is_dt = self.node_in_html(node_id, "dt");
                    if is_dd || is_dt {
                        let except = if is_dd { "dd" } else { "dt" };
                        self.generate_implied_end_tags(Some(except));
                        while let Some(popped) = self.pop_open_element() {
                            if popped == node_id {
                                break;
                            }
                        }
                        break;
                    }
                    if self.is_node_special(node_id)
                        && !matches!(
                            self.doc
                                .node(node_id)
                                .as_element()
                                .map(|el| (el.namespace, el.local_name.as_str())),
                            Some((Namespace::Html, "address" | "div" | "p"))
                        )
                    {
                        break;
                    }
                }
                if self.has_element_in_button_scope("p") {
                    self.close_p_element();
                }
                self.insert_html_element(name, attributes);
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "table" => {
                if self.doc.quirks_mode != QuirksMode::Quirks
                    && self.has_element_in_button_scope("p")
                {
                    self.close_p_element();
                }
                self.insert_html_element("table", attributes);
                self.frameset_ok = false;
                self.mode = InsertionMode::InTable;
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "plaintext" => {
                if self.has_element_in_button_scope("p") {
                    self.close_p_element();
                }
                self.insert_html_element("plaintext", attributes);
                Some(TokenizerState::Plaintext)
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "frameset" => {
                if self.open_elements.len() <= 1
                    || !self.node_in_html(self.open_elements[1], "body")
                    || !self.frameset_ok
                {
                    return None;
                }
                let body_id = self.open_elements[1];
                self.doc.detach(body_id);
                while self.open_elements.len() > 1 {
                    self.pop_open_element();
                }
                self.insert_html_element("frameset", attributes);
                self.mode = InsertionMode::InFrameset;
                None
            }
            Token::StartTag { ref name, .. }
                if matches!(
                    name.as_str(),
                    "caption"
                        | "col"
                        | "colgroup"
                        | "frame"
                        | "head"
                        | "tbody"
                        | "td"
                        | "tfoot"
                        | "th"
                        | "thead"
                        | "tr"
                ) =>
            {
                // Parse error. Ignore the token.
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "input" => {
                if let Some((_, ref ctx_name)) = self.fragment_context
                    && ctx_name == "select"
                {
                    return None;
                }
                if self.has_element_in_scope("select") {
                    while let Some(popped) = self.pop_open_element() {
                        if self.node_in_html(popped, "select") {
                            break;
                        }
                    }
                }
                self.reconstruct_active_formatting_elements();
                self.insert_html_element("input", attributes);
                self.pop_open_element();
                let is_hidden = attributes.iter().any(|(k, v)| {
                    k.eq_ignore_ascii_case("type") && v.eq_ignore_ascii_case("hidden")
                });
                if !is_hidden {
                    self.frameset_ok = false;
                }
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if VOID_ELEMENTS.contains(&name.as_str()) => {
                if name == "hr" {
                    if self.has_element_in_button_scope("p") {
                        self.close_p_element();
                    }
                    if self.has_element_in_scope("select") {
                        self.generate_implied_end_tags(None);
                    }
                } else {
                    self.reconstruct_active_formatting_elements();
                }
                self.insert_html_element(name, attributes);
                self.pop_open_element();
                if !matches!(name.as_str(), "param" | "source" | "track") {
                    self.frameset_ok = false;
                }
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if matches!(name.as_str(), "applet" | "marquee" | "object") => {
                self.reconstruct_active_formatting_elements();
                self.insert_html_element(name, attributes);
                self.active_formatting.push(FormattingEntry::Marker);
                self.frameset_ok = false;
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if matches!(name.as_str(), "rb" | "rtc") => {
                if self.has_element_in_scope("ruby") {
                    self.generate_implied_end_tags(None);
                }
                self.insert_html_element(name, attributes);
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if matches!(name.as_str(), "rp" | "rt") => {
                if self.has_element_in_scope("ruby") {
                    self.generate_implied_end_tags(Some("rtc"));
                }
                self.insert_html_element(name, attributes);
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "form" => {
                let parsing_template =
                    !self.template_modes.is_empty() || self.has_open_html_element("template");
                if self.form_element.is_some() && !parsing_template {
                    return None;
                }
                if self.has_element_in_button_scope("p") {
                    self.close_p_element();
                }
                let form_id = self.insert_html_element("form", attributes);
                if !parsing_template {
                    self.form_element = Some(form_id);
                }
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "button" => {
                if self.has_element_in_scope("button") {
                    self.generate_implied_end_tags(None);
                    while let Some(popped) = self.pop_open_element() {
                        if self.node_in_html(popped, "button") {
                            break;
                        }
                    }
                }
                self.reconstruct_active_formatting_elements();
                self.insert_html_element("button", attributes);
                self.frameset_ok = false;
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if P_CLOSING_BLOCK_ELEMENTS.contains(&name.as_str()) => {
                if self.has_element_in_button_scope("p") {
                    self.close_p_element();
                }
                self.insert_html_element(name, attributes);
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "a" => {
                let mut existing_a = None;
                for entry in self.active_formatting.iter().rev() {
                    match entry {
                        FormattingEntry::Marker => break,
                        FormattingEntry::Element {
                            node_id, tag_name, ..
                        } if tag_name == "a" => {
                            existing_a = Some(*node_id);
                            break;
                        }
                        _ => {}
                    }
                }
                if let Some(a_id) = existing_a {
                    self.run_adoption_agency_algorithm("a");
                    if let Some(pos) = self.active_formatting.iter().rposition(|e| {
                        matches!(e, FormattingEntry::Element { node_id, .. } if *node_id == a_id)
                    }) {
                        self.active_formatting.remove(pos);
                    }
                    if let Some(pos) = self.open_elements.iter().rposition(|&id| id == a_id) {
                        self.remove_open_element(pos);
                    }
                }
                self.reconstruct_active_formatting_elements();
                let id = self.insert_html_element(name, attributes);
                self.push_active_formatting(id, name, attributes);
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "nobr" => {
                self.reconstruct_active_formatting_elements();
                if self.has_element_in_scope("nobr") {
                    self.run_adoption_agency_algorithm("nobr");
                    self.reconstruct_active_formatting_elements();
                }
                let id = self.insert_html_element(name, attributes);
                self.push_active_formatting(id, name, attributes);
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if FORMATTING_ELEMENTS.contains(&name.as_str()) => {
                self.reconstruct_active_formatting_elements();
                let id = self.insert_html_element(name, attributes);
                self.push_active_formatting(id, name, attributes);
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                self_closing,
            } if name == "svg" || name == "math" => {
                let ns = if name == "svg" {
                    Namespace::Svg
                } else {
                    Namespace::MathMl
                };
                self.reconstruct_active_formatting_elements();
                self.insert_foreign_element(ns, name.clone(), attributes);
                if self_closing {
                    self.pop_open_element();
                }
                None
            }
            Token::StartTag {
                name,
                attributes,
                self_closing,
            } if name == "image" => self.process_in_body(Token::StartTag {
                name: "img".to_owned(),
                attributes,
                self_closing,
            }),
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "textarea" => {
                self.insert_html_element("textarea", attributes);
                self.skip_next_lf = true;
                self.original_mode = self.mode;
                self.frameset_ok = false;
                self.mode = InsertionMode::Text;
                Some(TokenizerState::Rcdata)
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "xmp" => {
                if self.has_element_in_button_scope("p") {
                    self.close_p_element();
                }
                self.reconstruct_active_formatting_elements();
                self.frameset_ok = false;
                self.insert_html_element("xmp", attributes);
                self.original_mode = self.mode;
                self.mode = InsertionMode::Text;
                Some(TokenizerState::Rawtext)
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "iframe" => {
                self.frameset_ok = false;
                self.insert_html_element("iframe", attributes);
                self.original_mode = self.mode;
                self.mode = InsertionMode::Text;
                Some(TokenizerState::Rawtext)
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "noembed" => {
                self.insert_html_element("noembed", attributes);
                self.original_mode = self.mode;
                self.mode = InsertionMode::Text;
                Some(TokenizerState::Rawtext)
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "noscript" && self.scripting_enabled => {
                self.insert_html_element("noscript", attributes);
                self.original_mode = self.mode;
                self.mode = InsertionMode::Text;
                Some(TokenizerState::Rawtext)
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "select" => {
                if let Some((_, ref ctx_name)) = self.fragment_context
                    && ctx_name == "select"
                {
                    return None;
                }
                if self.has_element_in_scope("select") {
                    while let Some(popped) = self.pop_open_element() {
                        if self.node_in_html(popped, "select") {
                            break;
                        }
                    }
                } else {
                    self.reconstruct_active_formatting_elements();
                    self.insert_html_element("select", attributes);
                    self.frameset_ok = false;
                }
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "optgroup" => {
                if self.has_element_in_scope("select") {
                    self.generate_implied_end_tags(None);
                } else if let Some(curr) = self.current_node()
                    && self.node_in_html(curr, "option")
                {
                    self.pop_open_element();
                }
                self.reconstruct_active_formatting_elements();
                self.insert_html_element("optgroup", attributes);
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "option" => {
                if self.has_element_in_scope("select") {
                    self.generate_implied_end_tags(Some("optgroup"));
                } else if let Some(curr) = self.current_node()
                    && self.node_in_html(curr, "option")
                {
                    self.pop_open_element();
                }
                self.reconstruct_active_formatting_elements();
                self.insert_html_element("option", attributes);
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } => {
                self.reconstruct_active_formatting_elements();
                self.insert_html_element(name, attributes);
                None
            }
            Token::EndTag { ref name, .. } if name == "body" => {
                if self.has_element_in_scope("body") {
                    self.mode = InsertionMode::AfterBody;
                }
                None
            }
            Token::EndTag { ref name, .. } if name == "html" => {
                if self.has_element_in_scope("body") {
                    self.mode = InsertionMode::AfterBody;
                    return self.process_token(token);
                }
                None
            }
            Token::EndTag { ref name, .. } if name == "p" => {
                if !self.has_element_in_button_scope("p") {
                    self.insert_html_element("p", &[]);
                }
                self.close_p_element();
                None
            }
            Token::EndTag { ref name, .. } if name == "li" => {
                if self.has_element_in_list_item_scope("li") {
                    self.generate_implied_end_tags(Some("li"));
                    while let Some(popped) = self.pop_open_element() {
                        if self.node_in_html(popped, "li") {
                            break;
                        }
                    }
                }
                None
            }
            Token::EndTag { ref name, .. } if matches!(name.as_str(), "dd" | "dt") => {
                if self.has_element_in_scope(name) {
                    self.generate_implied_end_tags(Some(name));
                    while let Some(popped) = self.pop_open_element() {
                        if self.node_in_html(popped, name) {
                            break;
                        }
                    }
                }
                None
            }
            Token::EndTag { ref name, .. }
                if matches!(name.as_str(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6") =>
            {
                let any_heading_in_scope = ["h1", "h2", "h3", "h4", "h5", "h6"]
                    .iter()
                    .any(|&h| self.has_element_in_scope(h));
                if any_heading_in_scope {
                    self.generate_implied_end_tags(None);
                    while let Some(popped) = self.pop_open_element() {
                        if self.doc.node(popped).as_element().is_some_and(|el| {
                            el.namespace == Namespace::Html
                                && matches!(
                                    el.local_name.as_str(),
                                    "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
                                )
                        }) {
                            break;
                        }
                    }
                }
                None
            }
            Token::EndTag { ref name, .. } if name == "form" => {
                let parsing_template =
                    !self.template_modes.is_empty() || self.has_open_html_element("template");
                if !parsing_template {
                    let node = self.form_element.take();
                    let node_id = node?;
                    if !self.has_node_in_scope(node_id) {
                        return None;
                    }
                    self.generate_implied_end_tags(None);
                    if let Some(pos) = self.open_elements.iter().rposition(|&id| id == node_id) {
                        self.remove_open_element(pos);
                    }
                } else {
                    if !self.has_element_in_scope("form") {
                        return None;
                    }
                    self.generate_implied_end_tags(None);
                    while let Some(popped) = self.pop_open_element() {
                        if self.node_in_html(popped, "form") {
                            break;
                        }
                    }
                }
                None
            }
            Token::EndTag { ref name, .. } if BLOCK_END_TAG_ELEMENTS.contains(&name.as_str()) => {
                if !self.has_element_in_scope(name) {
                    return None;
                }
                self.generate_implied_end_tags(None);
                while let Some(popped) = self.pop_open_element() {
                    if self.node_in_html(popped, name) {
                        break;
                    }
                }
                None
            }
            Token::EndTag { ref name, .. }
                if matches!(name.as_str(), "applet" | "marquee" | "object") =>
            {
                if !self.has_element_in_scope(name) {
                    return None;
                }
                self.generate_implied_end_tags(None);
                while let Some(popped) = self.pop_open_element() {
                    if self.node_in_html(popped, name) {
                        break;
                    }
                }
                self.clear_active_formatting_to_last_marker();
                None
            }
            Token::EndTag { ref name, .. } if name == "br" => {
                self.reconstruct_active_formatting_elements();
                self.insert_html_element("br", &[]);
                self.pop_open_element();
                self.frameset_ok = false;
                None
            }
            Token::EndTag { ref name, .. } if FORMATTING_ELEMENTS.contains(&name.as_str()) => {
                self.run_adoption_agency_algorithm(name);
                None
            }
            Token::EndTag { ref name, .. } => {
                self.process_any_other_end_tag(name);
                None
            }
            Token::Eof => None,
        }
    }

    fn is_table_text_current_node(&self) -> bool {
        self.current_node().is_some_and(|id| {
            self.doc.node(id).as_element().is_some_and(|el| {
                el.namespace == Namespace::Html
                    && matches!(
                        el.local_name.as_str(),
                        "table" | "tbody" | "template" | "tfoot" | "thead" | "tr"
                    )
            })
        })
    }

    fn process_in_table_text(&mut self, token: Token) -> Option<TokenizerState> {
        match token {
            Token::Character(0x00) => None,
            Token::Character(ch) => {
                self.pending_table_characters
                    .push(char::from_u32(ch).unwrap_or('\u{FFFD}'));
                None
            }
            other => {
                let has_non_whitespace = self
                    .pending_table_characters
                    .iter()
                    .any(|&c| !matches!(c, '\t' | '\n' | '\x0C' | '\r' | ' '));

                let chars = std::mem::take(&mut self.pending_table_characters);
                if has_non_whitespace {
                    let prev_fp = self.foster_parenting;
                    self.foster_parenting = true;
                    for c in chars {
                        self.process_in_body(Token::Character(c as u32));
                    }
                    self.foster_parenting = prev_fp;
                } else {
                    for c in chars {
                        self.insert_character(c as u32);
                    }
                }
                self.mode = self.original_mode;
                self.process_token(other)
            }
        }
    }

    fn process_in_table(&mut self, token: Token) -> Option<TokenizerState> {
        match token {
            Token::Character(_) if self.is_table_text_current_node() => {
                self.pending_table_characters.clear();
                self.original_mode = self.mode;
                self.mode = InsertionMode::InTableText;
                self.process_in_table_text(token)
            }
            Token::Comment(data) => {
                self.insert_comment(&data, None);
                None
            }
            Token::ProcessingInstruction { target, data } => {
                self.insert_processing_instruction(&target, &data);
                None
            }
            Token::Doctype { .. } => None,
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "caption" => {
                self.clear_stack_back_to_table_context();
                self.active_formatting.push(FormattingEntry::Marker);
                self.insert_html_element("caption", attributes);
                self.mode = InsertionMode::InCaption;
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "colgroup" => {
                self.clear_stack_back_to_table_context();
                self.insert_html_element("colgroup", attributes);
                self.mode = InsertionMode::InColumnGroup;
                None
            }
            Token::StartTag { ref name, .. } if name == "col" => {
                self.clear_stack_back_to_table_context();
                self.insert_html_element("colgroup", &[]);
                self.mode = InsertionMode::InColumnGroup;
                self.process_token(token)
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if matches!(name.as_str(), "tbody" | "tfoot" | "thead") => {
                self.clear_stack_back_to_table_context();
                self.insert_html_element(name, attributes);
                self.mode = InsertionMode::InTableBody;
                None
            }
            Token::StartTag { ref name, .. } if matches!(name.as_str(), "td" | "th" | "tr") => {
                self.clear_stack_back_to_table_context();
                self.insert_html_element("tbody", &[]);
                self.mode = InsertionMode::InTableBody;
                self.process_token(token)
            }
            Token::StartTag { ref name, .. } if name == "table" => {
                if self.has_element_in_table_scope("table") {
                    while let Some(popped) = self.pop_open_element() {
                        if self.node_in_html(popped, "table") {
                            break;
                        }
                    }
                    self.reset_insertion_mode_appropriately();
                    return self.process_token(token);
                }
                None
            }
            Token::EndTag { ref name, .. } if name == "table" => {
                if self.has_element_in_table_scope("table") {
                    while let Some(popped) = self.pop_open_element() {
                        if self.node_in_html(popped, "table") {
                            break;
                        }
                    }
                    self.reset_insertion_mode_appropriately();
                }
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "form" => {
                let parsing_template = self.has_open_html_element("template");
                if self.form_element.is_some() && !parsing_template {
                    return None;
                }
                let form_id = self.insert_html_element("form", attributes);
                if !parsing_template {
                    self.form_element = Some(form_id);
                }
                self.pop_open_element();
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "input"
                && attributes.iter().any(|(k, v)| {
                    k.eq_ignore_ascii_case("type") && v.eq_ignore_ascii_case("hidden")
                }) =>
            {
                self.insert_html_element("input", attributes);
                self.pop_open_element();
                None
            }
            Token::StartTag { ref name, .. }
                if matches!(name.as_str(), "style" | "script" | "template") =>
            {
                self.process_in_head(token)
            }
            Token::EndTag { ref name, .. } if name == "template" => self.process_in_head(token),
            Token::Eof => self.process_in_body(Token::Eof),
            other => {
                // Foster parenting for anything else in table!
                let prev = self.foster_parenting;
                self.foster_parenting = true;
                let res = self.process_in_body(other);
                self.foster_parenting = prev;
                res
            }
        }
    }

    fn process_in_caption(&mut self, token: Token) -> Option<TokenizerState> {
        match token {
            Token::EndTag { ref name, .. } if name == "caption" => {
                if !self.has_element_in_table_scope("caption") {
                    return None;
                }
                self.generate_implied_end_tags(None);
                while let Some(popped) = self.pop_open_element() {
                    if self.node_in_html(popped, "caption") {
                        break;
                    }
                }
                self.clear_active_formatting_to_last_marker();
                self.mode = InsertionMode::InTable;
                None
            }
            Token::StartTag { ref name, .. }
                if matches!(
                    name.as_str(),
                    "caption"
                        | "col"
                        | "colgroup"
                        | "tbody"
                        | "td"
                        | "tfoot"
                        | "th"
                        | "thead"
                        | "tr"
                ) =>
            {
                if !self.has_element_in_table_scope("caption") {
                    return None;
                }
                self.generate_implied_end_tags(None);
                while let Some(popped) = self.pop_open_element() {
                    if self.node_in_html(popped, "caption") {
                        break;
                    }
                }
                self.clear_active_formatting_to_last_marker();
                self.mode = InsertionMode::InTable;
                self.process_token(token)
            }
            Token::EndTag { ref name, .. } if name == "table" => {
                if !self.has_element_in_table_scope("caption") {
                    return None;
                }
                self.generate_implied_end_tags(None);
                while let Some(popped) = self.pop_open_element() {
                    if self.node_in_html(popped, "caption") {
                        break;
                    }
                }
                self.clear_active_formatting_to_last_marker();
                self.mode = InsertionMode::InTable;
                self.process_token(token)
            }
            Token::EndTag { ref name, .. }
                if matches!(
                    name.as_str(),
                    "body"
                        | "col"
                        | "colgroup"
                        | "html"
                        | "tbody"
                        | "td"
                        | "tfoot"
                        | "th"
                        | "thead"
                        | "tr"
                ) =>
            {
                None
            }
            other => self.process_in_body(other),
        }
    }

    fn process_in_column_group(&mut self, token: Token) -> Option<TokenizerState> {
        match token {
            Token::Character(cp @ (0x09 | 0x0A | 0x0C | 0x0D | 0x20)) => {
                self.insert_character(cp);
                None
            }
            Token::Comment(data) => {
                self.insert_comment(&data, None);
                None
            }
            Token::ProcessingInstruction { target, data } => {
                self.insert_processing_instruction(&target, &data);
                None
            }
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "col" => {
                self.insert_html_element("col", attributes);
                self.pop_open_element();
                None
            }
            Token::EndTag { ref name, .. } if name == "colgroup" => {
                if self
                    .current_node()
                    .is_some_and(|id| self.node_in_html(id, "colgroup"))
                {
                    self.pop_open_element();
                    self.mode = InsertionMode::InTable;
                }
                None
            }
            Token::StartTag { ref name, .. } if name == "template" => self.process_in_head(token),
            Token::EndTag { ref name, .. } if name == "template" => self.process_in_head(token),
            other => {
                if self
                    .current_node()
                    .is_some_and(|id| self.node_in_html(id, "colgroup"))
                {
                    self.pop_open_element();
                    self.mode = InsertionMode::InTable;
                    return self.process_token(other);
                }
                None
            }
        }
    }

    fn process_in_table_body(&mut self, token: Token) -> Option<TokenizerState> {
        match token {
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if name == "tr" => {
                self.clear_stack_back_to_table_body_context();
                self.insert_html_element("tr", attributes);
                self.mode = InsertionMode::InRow;
                None
            }
            Token::StartTag { ref name, .. } if matches!(name.as_str(), "th" | "td") => {
                self.clear_stack_back_to_table_body_context();
                self.insert_html_element("tr", &[]);
                self.mode = InsertionMode::InRow;
                self.process_token(token)
            }
            Token::EndTag { ref name, .. }
                if matches!(name.as_str(), "tbody" | "tfoot" | "thead") =>
            {
                if self.has_element_in_table_scope(name) {
                    self.clear_stack_back_to_table_body_context();
                    self.pop_open_element();
                    self.mode = InsertionMode::InTable;
                }
                None
            }
            Token::StartTag { ref name, .. }
                if matches!(
                    name.as_str(),
                    "caption" | "col" | "colgroup" | "tbody" | "tfoot" | "thead"
                ) =>
            {
                if !self.has_table_body_in_table_scope() {
                    return None;
                }
                self.clear_stack_back_to_table_body_context();
                self.pop_open_element();
                self.mode = InsertionMode::InTable;
                self.process_token(token)
            }
            Token::EndTag { ref name, .. } if name == "table" => {
                if !self.has_table_body_in_table_scope() {
                    return None;
                }
                self.clear_stack_back_to_table_body_context();
                self.pop_open_element();
                self.mode = InsertionMode::InTable;
                self.process_token(token)
            }
            other => self.process_in_table(other),
        }
    }

    fn process_in_row(&mut self, token: Token) -> Option<TokenizerState> {
        match token {
            Token::StartTag {
                ref name,
                ref attributes,
                ..
            } if matches!(name.as_str(), "th" | "td") => {
                self.clear_stack_back_to_table_row_context();
                self.insert_html_element(name, attributes);
                self.mode = InsertionMode::InCell;
                self.active_formatting.push(FormattingEntry::Marker);
                None
            }
            Token::EndTag { ref name, .. } if name == "tr" => {
                if self.has_element_in_table_scope("tr") {
                    self.clear_stack_back_to_table_row_context();
                    self.pop_open_element();
                    self.mode = InsertionMode::InTableBody;
                }
                None
            }
            Token::StartTag { ref name, .. }
                if matches!(
                    name.as_str(),
                    "caption" | "col" | "colgroup" | "tbody" | "tfoot" | "thead" | "tr"
                ) =>
            {
                if self.has_element_in_table_scope("tr") {
                    self.clear_stack_back_to_table_row_context();
                    self.pop_open_element();
                    self.mode = InsertionMode::InTableBody;
                    return self.process_token(token);
                }
                None
            }
            Token::EndTag { ref name, .. }
                if matches!(name.as_str(), "tbody" | "tfoot" | "thead") =>
            {
                if self.has_element_in_table_scope(name) && self.has_element_in_table_scope("tr") {
                    self.clear_stack_back_to_table_row_context();
                    self.pop_open_element();
                    self.mode = InsertionMode::InTableBody;
                    return self.process_token(token);
                }
                None
            }
            Token::EndTag { ref name, .. } if name == "table" => {
                if self.has_element_in_table_scope("tr") {
                    self.clear_stack_back_to_table_row_context();
                    self.pop_open_element();
                    self.mode = InsertionMode::InTableBody;
                    return self.process_token(token);
                }
                None
            }
            other => self.process_in_table(other),
        }
    }

    fn close_cell(&mut self) {
        self.generate_implied_end_tags(None);
        while let Some(popped) = self.pop_open_element() {
            if self.node_in_html(popped, "td") || self.node_in_html(popped, "th") {
                break;
            }
        }
        while let Some(entry) = self.active_formatting.pop() {
            if entry == FormattingEntry::Marker {
                break;
            }
        }
        self.mode = InsertionMode::InRow;
    }

    fn process_in_cell(&mut self, token: Token) -> Option<TokenizerState> {
        match token {
            Token::EndTag { ref name, .. } if matches!(name.as_str(), "td" | "th") => {
                if self.has_element_in_table_scope(name) {
                    self.close_cell();
                }
                None
            }
            Token::StartTag { ref name, .. }
                if matches!(
                    name.as_str(),
                    "caption"
                        | "col"
                        | "colgroup"
                        | "tbody"
                        | "td"
                        | "tfoot"
                        | "th"
                        | "thead"
                        | "tr"
                ) =>
            {
                if self.has_element_in_table_scope("td") || self.has_element_in_table_scope("th") {
                    self.close_cell();
                    return self.process_token(token);
                }
                None
            }
            Token::EndTag { ref name, .. }
                if matches!(name.as_str(), "table" | "tbody" | "tfoot" | "thead" | "tr") =>
            {
                if self.has_element_in_table_scope(name) {
                    self.close_cell();
                    return self.process_token(token);
                }
                None
            }
            other => self.process_in_body(other),
        }
    }

    /// Collects, in document order, the descendants of `parent` named `name` (HTML
    /// namespace), *not* descending into nested `<select>` elements.
    ///
    /// Iterative on purpose: the previous recursive implementation overflowed the call
    /// stack (SIGABRT) on ~100k nested elements, and this fixup runs on **every** parse
    /// (audit finding F1). The explicit heap stack keeps memory `O(nodes)` worst case
    /// but never grows the call stack.
    fn collect_descendants_within_select(&self, parent: NodeId, name: &str, out: &mut Vec<NodeId>) {
        let mut stack = vec![parent];
        while let Some(current) = stack.pop() {
            if current != parent && self.node_in_html(current, name) {
                out.push(current);
            }
            // The recursive walk never descended into nested `<select>` elements: match
            // that by visiting the select itself but not its children. (`parent` is the
            // select itself in the `<selectedcontent>`/`<option>` queries, so it is always
            // descended into.)
            if current != parent && self.node_in_html(current, "select") {
                continue;
            }
            // Collect children, then push them reversed so they are popped in document
            // order (pre-order DFS, same order as the previous recursive walk).
            let mut children = Vec::new();
            let mut curr = self.doc.node(current).first_child;
            while let Some(child_id) = curr {
                children.push(child_id);
                curr = self.doc.node(child_id).next_sibling;
            }
            for child_id in children.into_iter().rev() {
                stack.push(child_id);
            }
        }
    }

    pub fn update_selectedcontent_elements(&mut self) {
        // No `<select>` was ever inserted: the `<selectedcontent>` fixup cannot possibly
        // do anything, so skip the whole-tree walk (this is what runs on every parse).
        if !self.has_select_element {
            return;
        }
        let root = self.doc.root();
        let mut selects = Vec::new();
        self.collect_descendants_within_select(root, "select", &mut selects);

        for select_id in selects {
            let mut selectedcontents = Vec::new();
            self.collect_descendants_within_select(
                select_id,
                "selectedcontent",
                &mut selectedcontents,
            );
            if selectedcontents.is_empty() {
                continue;
            }

            let mut options = Vec::new();
            self.collect_descendants_within_select(select_id, "option", &mut options);
            if options.is_empty() {
                continue;
            }

            let selected_option = options
                .iter()
                .copied()
                .rfind(|&opt_id| {
                    if let Some(el) = self.doc.node(opt_id).as_element() {
                        el.attributes.iter().any(|a| a.local_name == "selected")
                    } else {
                        false
                    }
                })
                .unwrap_or(options[0]);

            for sc_id in selectedcontents {
                while let Some(child) = self.doc.node(sc_id).first_child {
                    self.doc.detach(child);
                }
                let mut curr = self.doc.node(selected_option).first_child;
                while let Some(child_id) = curr {
                    let next = self.doc.node(child_id).next_sibling;
                    let cloned = self.doc.clone_node(child_id, true);
                    self.doc.append_child(sc_id, cloned);
                    curr = next;
                }
            }
        }
    }
}

/// Parses a complete HTML document string into a [`dom::Document`] (§13.2) using the
/// default anti-DoS [`ParseOptions`].
///
/// The signature is unchanged for callers that only need the tree; use
/// [`parse_document_with_options`] when the limit outcomes must be inspected.
pub fn parse_document(input: &str, scripting_enabled: bool) -> Document {
    parse_document_with_options(input, scripting_enabled, &ParseOptions::default()).document
}

/// Parses a complete HTML document string with explicit anti-DoS [`ParseOptions`].
///
/// The parser is a pure function of `(truncated input, options)`: when a hard limit is
/// exceeded the offending tokens are ignored and counted in
/// [`ParseOutcome::limit_violations`], so hostile input can never make this function
/// panic, abort, recurse without bound, or tokenize more than `max_input_bytes`.
pub fn parse_document_with_options(
    input: &str,
    scripting_enabled: bool,
    options: &ParseOptions,
) -> ParseOutcome {
    parse_document_with_options_and_executor(input, scripting_enabled, options, None)
}

/// Parses a complete HTML document with a host-provided **parser-inserted script executor**
/// (WP-6.4). See [`crate::scripting`] for the documented semantics.
///
/// `script_executor = None` behaves exactly like [`parse_document_with_options`] (the
/// zero-cost path: no script is executed and the tokenizer is untouched).
pub fn parse_document_with_options_and_executor(
    input: &str,
    scripting_enabled: bool,
    options: &ParseOptions,
    script_executor: Option<Box<dyn crate::scripting::ScriptExecutor>>,
) -> ParseOutcome {
    let (input, input_truncated) = truncate_input(input, options.max_input_bytes);
    let mut tok = Tokenizer::from_str_input(input, TokenizerConfig::default());
    let mut tb = TreeBuilder::with_options(scripting_enabled, *options);
    tb.limit_violations.input_truncated = input_truncated;
    tb.script_executor = script_executor;

    drive_tokenizer(&mut tok, &mut tb);

    // Ensure `<html>`, `<head>`, and `<body>` exist if parsing ended before `InBody`.
    // If a parser-inserted script detached the document element during the parse, the
    // synthetic skeleton must NOT be re-created (`scripted_foster01`): the fixture expects
    // the post-script document (e.g. a lone detached table at the document root).
    let root = tb.doc.root();
    let has_html = tb.doc.children(root).any(|id| tb.node_in_html(id, "html"));
    if !tb.html_element_seen
        && !has_html
        && tb.mode != InsertionMode::InFrameset
        && tb.mode != InsertionMode::AfterFrameset
    {
        let html_id = tb.doc.create_element(Namespace::Html, "html", vec![]);
        let head_id = tb.doc.create_element(Namespace::Html, "head", vec![]);
        let body_id = tb.doc.create_element(Namespace::Html, "body", vec![]);
        tb.doc.append_child(root, html_id);
        tb.doc.append_child(html_id, head_id);
        tb.doc.append_child(html_id, body_id);
    } else if let Some(html_id) = tb
        .doc
        .children(root)
        .find(|&id| tb.node_in_html(id, "html"))
    {
        let has_head = tb
            .doc
            .children(html_id)
            .any(|id| tb.node_in_html(id, "head"));
        let has_body_or_frameset = tb
            .doc
            .children(html_id)
            .any(|id| tb.node_in_html(id, "body") || tb.node_in_html(id, "frameset"));
        if !has_head {
            let head_id = tb.doc.create_element(Namespace::Html, "head", vec![]);
            tb.doc.append_child(html_id, head_id);
        }
        if !has_body_or_frameset {
            let body_id = tb.doc.create_element(Namespace::Html, "body", vec![]);
            tb.doc.append_child(html_id, body_id);
        }
    }

    tb.update_selectedcontent_elements();
    let TreeBuilder {
        doc,
        limit_violations,
        script_errors,
        ..
    } = tb;
    ParseOutcome {
        document: doc,
        limit_violations,
        script_errors,
    }
}

/// Convenience wrapper for [`parse_document_with_options_and_executor`] using the default
/// anti-DoS [`ParseOptions`].
pub fn parse_document_with_script_executor(
    input: &str,
    scripting_enabled: bool,
    executor: Box<dyn ScriptExecutor>,
) -> ParseOutcome {
    parse_document_with_options_and_executor(
        input,
        scripting_enabled,
        &ParseOptions::default(),
        Some(executor),
    )
}

/// Parses an HTML fragment in the context of `(context_namespace, context_local_name)` (§13.4)
/// and returns a [`dom::Document`] whose root contains the resulting fragment children,
/// using the default anti-DoS [`ParseOptions`].
pub fn parse_fragment(
    input: &str,
    context_namespace: Namespace,
    context_local_name: &str,
    scripting_enabled: bool,
) -> Document {
    parse_fragment_with_options(
        input,
        context_namespace,
        context_local_name,
        scripting_enabled,
        &ParseOptions::default(),
    )
    .document
}

/// Parses an HTML fragment with explicit anti-DoS [`ParseOptions`]; see
/// [`parse_fragment`] and [`parse_document_with_options`].
pub fn parse_fragment_with_options(
    input: &str,
    context_namespace: Namespace,
    context_local_name: &str,
    scripting_enabled: bool,
    options: &ParseOptions,
) -> ParseOutcome {
    let tb = TreeBuilder::with_options(scripting_enabled, *options);
    let tb = run_fragment_parser(
        tb,
        input,
        context_namespace,
        context_local_name,
        scripting_enabled,
        options,
    );
    let TreeBuilder {
        doc,
        limit_violations,
        script_errors,
        ..
    } = tb;
    ParseOutcome {
        document: doc,
        limit_violations,
        script_errors,
    }
}

/// Parses an HTML fragment **into an existing [`Document`]**, appending the resulting
/// children to `doc.root()` (WHATWG HTML §13.4 fragment parsing).
///
/// This is the in-place building block used by [`crate::set_inner_html`],
/// [`crate::set_outer_html`] and [`crate::document_write`]: it avoids creating a temporary
/// document and copying nodes between arenas. Existing document content is left untouched;
/// the new fragment children are appended as the last children of the document root (the
/// caller usually moves them under the target element). Returns the
/// [`LimitViolations`] hit while parsing the fragment.
pub fn parse_fragment_into(
    doc: &mut Document,
    input: &str,
    context_namespace: Namespace,
    context_local_name: &str,
    scripting_enabled: bool,
    options: &ParseOptions,
) -> LimitViolations {
    let existing = std::mem::take(doc);
    let tb = TreeBuilder::with_document(scripting_enabled, *options, existing);
    let tb = run_fragment_parser(
        tb,
        input,
        context_namespace,
        context_local_name,
        scripting_enabled,
        options,
    );
    let TreeBuilder {
        doc: parsed,
        limit_violations,
        ..
    } = tb;
    *doc = parsed;
    limit_violations
}

/// Shared fragment parser: tokenizer setup, §13.4 synthetic `<html>` root, token loop and
/// finalization (move children to the document root, drop the synthetic root/context node).
fn run_fragment_parser(
    mut tb: TreeBuilder,
    input: &str,
    context_namespace: Namespace,
    context_local_name: &str,
    scripting_enabled: bool,
    options: &ParseOptions,
) -> TreeBuilder {
    let (input, input_truncated) = truncate_input(input, options.max_input_bytes);
    tb.limit_violations.input_truncated = input_truncated;
    let initial_tok_state = if context_namespace == Namespace::Html {
        match context_local_name {
            "title" | "textarea" => TokenizerState::Rcdata,
            "style" | "xmp" | "iframe" | "noembed" | "noframes" => TokenizerState::Rawtext,
            "script" => TokenizerState::ScriptData,
            "noscript" if scripting_enabled => TokenizerState::Rawtext,
            "plaintext" => TokenizerState::Plaintext,
            _ => TokenizerState::Data,
        }
    } else {
        TokenizerState::Data
    };

    let mut tok = Tokenizer::from_str_input(
        input,
        TokenizerConfig {
            initial_state: initial_tok_state,
            last_start_tag: None,
        },
    );

    tb.fragment_context = Some((context_namespace, context_local_name.to_owned()));
    let ctx_node = tb
        .doc
        .create_element(context_namespace, context_local_name, vec![]);
    tb.fragment_context_node = Some(ctx_node);

    let html_root = tb.doc.create_element(Namespace::Html, "html", vec![]);
    tb.doc.append_child(tb.doc.root(), html_root);
    tb.push_open_element(html_root);

    if context_namespace == Namespace::Html && context_local_name == "template" {
        tb.template_modes.push(InsertionMode::InTemplate);
    }
    tb.reset_insertion_mode_appropriately();

    drive_tokenizer(&mut tok, &mut tb);

    // Move children of the synthetic `<html>` root directly under `doc.root()` so callers
    // can pick them up as "the new children appended after the previous last child".
    let root = tb.doc.root();
    tb.doc.reparent_children(html_root, root);
    tb.doc.detach(html_root);
    // The synthetic `<html>` root and the fragment context element are parser scaffolding:
    // free their slots so repeated in-place fragment parses (`innerHTML`) do not leak arena
    // nodes. Neither is reachable from the returned document tree.
    tb.doc.remove_subtree(html_root);
    if let Some(ctx_id) = tb.fragment_context_node.take() {
        tb.doc.remove_subtree(ctx_id);
    }
    tb.update_selectedcontent_elements();
    tb
}

/// Runs the tokenizer → tree-builder loop until EOF.
/// Appends `cp` to a UTF-8 byte run (NUL/invalid code points are dropped, matching
/// [`TreeBuilder::process_character_run`]).
fn push_codepoint_utf8(out: &mut Vec<u8>, cp: u32) {
    if cp == 0 {
        return;
    }
    if cp < 0x80 {
        out.push(cp as u8);
    } else if let Some(ch) = char::from_u32(cp) {
        let mut buffer = [0u8; 4];
        out.extend_from_slice(ch.encode_utf8(&mut buffer).as_bytes());
    }
}

fn drive_tokenizer(tok: &mut Tokenizer, tb: &mut TreeBuilder) {
    // Reused token batch buffer: the old code allocated a `Vec` for every emitted token.
    let mut emitted: Vec<Token> = Vec::with_capacity(4);
    // The CDATA allowance only depends on the adjusted current node, which is determined by
    // `(open_elements.len(), current_node)` (plus the fixed fragment context). Caching the
    // key avoids two arena lookups per input character.
    let mut cdata_key: Option<(usize, Option<NodeId>)> = None;
    // Consecutive character tokens of the "in body" mode are processed as one run.
    let mut char_run: Vec<u8> = Vec::new();
    let refresh_cdata =
        |tok: &mut Tokenizer, tb: &TreeBuilder, key: &mut Option<(usize, Option<NodeId>)>| {
            let current_key = (tb.open_elements.len(), tb.current_node());
            if *key != Some(current_key) {
                tok.allow_cdata = tb.is_adjusted_current_node_foreign();
                *key = Some(current_key);
            }
        };
    loop {
        // Fast path: plain text in the "data" state can be consumed straight into the
        // character run without emitting/handling one `Token::Character` per code point.
        if tok.state == TokenizerState::Data
            && tb.mode == InsertionMode::InBody
            && tb.doc.len() < tb.parse_options.max_nodes
            && !tb.may_have_foreign_content()
        {
            let consumed = tok.consume_data_run(&mut char_run);
            if consumed > 0 {
                continue;
            }
        }
        refresh_cdata(tok, tb, &mut cdata_key);
        let prev_len = tok.tokens.len();
        tok.step();
        if tok.tokens.len() > prev_len {
            emitted.clear();
            emitted.extend(tok.tokens.drain(prev_len..));
            let mut saw_eof = false;
            for token in emitted.drain(..) {
                if let Token::Character(cp) = token {
                    if tb.mode == InsertionMode::InBody
                        && tb.doc.len() < tb.parse_options.max_nodes
                        && (!char_run.is_empty() || !tb.may_have_foreign_content())
                    {
                        push_codepoint_utf8(&mut char_run, cp);
                        continue;
                    }
                    if !char_run.is_empty() {
                        tb.process_character_run(&char_run);
                        char_run.clear();
                    }
                    if let Some(new_state) = tb.process_token(Token::Character(cp)) {
                        tok.state = new_state;
                    }
                    refresh_cdata(tok, tb, &mut cdata_key);
                    continue;
                }
                if !char_run.is_empty() {
                    tb.process_character_run(&char_run);
                    char_run.clear();
                }
                if token == Token::Eof {
                    saw_eof = true;
                }
                if let Some(new_state) = tb.process_token(token) {
                    tok.state = new_state;
                }
                // WP-6.4: a parser-inserted script may have written HTML with
                // `document.write`; re-tokenize it at the current position (immediately
                // after the token that closed the script, before the remaining input).
                if let Some(written) = tb.take_pending_written_html() {
                    tok.insert_input(&written);
                }
                refresh_cdata(tok, tb, &mut cdata_key);
            }
            if saw_eof {
                break;
            }
        }
    }
}

/// Truncates `input` to at most `max_bytes` bytes on a UTF-8 character boundary.
/// Returns the prefix and whether any truncation happened.
fn truncate_input(input: &str, max_bytes: usize) -> (&str, bool) {
    if input.len() <= max_bytes {
        return (input, false);
    }
    let mut end = max_bytes;
    while end > 0 && !input.is_char_boundary(end) {
        end -= 1;
    }
    (&input[..end], true)
}

#[cfg(test)]
mod robustness_regression_tests {
    use std::collections::{HashMap, HashSet};

    use super::*;

    /// Recomputes the incremental bookkeeping from the stack and compares it with the
    /// maintained counters/sets.
    fn assert_bookkeeping_consistent(tb: &TreeBuilder) {
        let mut expected: HashMap<String, u32> = HashMap::new();
        let mut expected_ids: HashSet<NodeId> = HashSet::new();
        for &node_id in &tb.open_elements {
            expected_ids.insert(node_id);
            if let Some(el) = tb.doc.node(node_id).as_element()
                && el.namespace == Namespace::Html
            {
                *expected.entry(el.local_name.clone()).or_insert(0) += 1;
            }
        }
        assert_eq!(
            tb.open_ids.len(),
            expected_ids.len(),
            "open_ids diverged from the stack"
        );
        assert!(
            expected_ids.iter().all(|id| tb.open_ids.contains(id)),
            "open_ids diverged from the stack"
        );
        assert_eq!(
            tb.open_html_counts.len(),
            expected.len(),
            "open_html_counts diverged from the stack"
        );
        assert!(
            expected
                .iter()
                .all(|(name, count)| tb.open_html_counts.get(name) == Some(count)),
            "open_html_counts diverged from the stack"
        );
    }

    #[test]
    fn test_open_element_bookkeeping_stays_consistent() {
        let mut tb = TreeBuilder::new(false);
        let div = tb.doc.create_element(Namespace::Html, "div", vec![]);
        let span = tb.doc.create_element(Namespace::Html, "span", vec![]);
        let svg = tb.doc.create_element(Namespace::Svg, "circle", vec![]);
        let tmpl = tb.doc.create_element(Namespace::Html, "template", vec![]);

        tb.push_open_element(div);
        tb.push_open_element(span);
        tb.push_open_element(svg); // foreign elements are not counted by tag name
        assert_bookkeeping_consistent(&tb);
        assert_eq!(tb.open_html_count("div"), 1);
        assert_eq!(tb.open_html_count("circle"), 0);

        assert_eq!(tb.pop_open_element(), Some(svg));
        assert_bookkeeping_consistent(&tb);

        // Arbitrary-position insert/remove used by the adoption agency.
        tb.insert_open_element(0, svg);
        tb.insert_open_element(1, tmpl);
        assert_bookkeeping_consistent(&tb);
        assert!(tb.has_open_html_element("template"));
        let removed = tb.remove_open_element(1);
        assert_eq!(removed, tmpl);
        assert_bookkeeping_consistent(&tb);
        assert!(!tb.has_open_html_element("template"));

        // Drain the stack: counters must return to zero (the maps must be empty again).
        while tb.pop_open_element().is_some() {}
        assert!(tb.open_elements.is_empty());
        assert!(tb.open_ids.is_empty());
        assert!(tb.open_html_counts.is_empty());
    }

    /// Audit finding F2 mentioned a latent `open_elements[fmt_stack_pos - 1]` underflow in
    /// the adoption agency: unreachable for parser-built trees (the formatting element is
    /// never bottom-most), but a manually built/corrupted stack must not panic in release.
    #[test]
    fn test_adoption_agency_does_not_underflow_on_bottom_most_formatting_element() {
        let mut tb = TreeBuilder::new(false);
        let bold = tb.doc.create_element(Namespace::Html, "b", vec![]);
        let div = tb.doc.create_element(Namespace::Html, "div", vec![]);
        // Pathological stack: the formatting element sits at position 0 and a special
        // element above it forces the "furthest block" branch (`fmt_stack_pos - 1`).
        tb.push_open_element(bold);
        tb.push_open_element(div);
        tb.active_formatting.push(FormattingEntry::Element {
            node_id: bold,
            tag_name: "b".to_owned(),
            attributes: Vec::new(),
        });
        tb.run_adoption_agency_algorithm("b");
        assert_bookkeeping_consistent(&tb);
    }

    /// The scope fast paths must never disagree with an independent unbounded scan.
    #[test]
    fn test_scope_fast_paths_agree_with_full_scan() {
        let mut tb = TreeBuilder::new(false);
        let div = tb.doc.create_element(Namespace::Html, "div", vec![]);
        let button = tb.doc.create_element(Namespace::Html, "button", vec![]);
        let table = tb.doc.create_element(Namespace::Html, "table", vec![]);
        let p = tb.doc.create_element(Namespace::Html, "p", vec![]);
        for id in [div, button, table, p] {
            tb.push_open_element(id);
        }

        let default_boundary =
            |el: &dom::ElementData| scope_boundary(el, DEFAULT_SCOPE_ELEMENTS, &[]);
        let button_boundary =
            |el: &dom::ElementData| scope_boundary(el, DEFAULT_SCOPE_ELEMENTS, &["button"]);
        let list_boundary =
            |el: &dom::ElementData| scope_boundary(el, DEFAULT_SCOPE_ELEMENTS, &["ol", "ul"]);
        let table_scope: &[(&str, Namespace)] = &[
            ("html", Namespace::Html),
            ("table", Namespace::Html),
            ("template", Namespace::Html),
        ];
        let table_boundary = |el: &dom::ElementData| scope_boundary(el, table_scope, &[]);

        for target in ["p", "div", "table", "li", "select", "html"] {
            assert_eq!(
                tb.has_element_in_scope(target),
                brute_force_scope(&tb, target, default_boundary),
                "in-scope mismatch for {target}"
            );
            assert_eq!(
                tb.has_element_in_button_scope(target),
                brute_force_scope(&tb, target, button_boundary),
                "button-scope mismatch for {target}"
            );
            assert_eq!(
                tb.has_element_in_list_item_scope(target),
                brute_force_scope(&tb, target, list_boundary),
                "list-item-scope mismatch for {target}"
            );
            assert_eq!(
                tb.has_element_in_table_scope(target),
                brute_force_scope(&tb, target, table_boundary),
                "table-scope mismatch for {target}"
            );
        }
    }

    fn scope_boundary(
        el: &dom::ElementData,
        scope_list: &[(&str, Namespace)],
        extra_html_tags: &[&str],
    ) -> bool {
        scope_list
            .iter()
            .any(|&(name, ns)| el.namespace == ns && el.local_name == name)
            || (el.namespace == Namespace::Html
                && extra_html_tags.contains(&el.local_name.as_str()))
    }

    /// Independent reference implementation of "has element in scope" (§13.2.4.2).
    fn brute_force_scope(
        tb: &TreeBuilder,
        target_name: &str,
        is_boundary: impl Fn(&dom::ElementData) -> bool,
    ) -> bool {
        for &node_id in tb.open_elements.iter().rev() {
            let Some(el) = tb.doc.node(node_id).as_element() else {
                continue;
            };
            if el.namespace == Namespace::Html && el.local_name == target_name {
                return true;
            }
            if is_boundary(el) {
                return false;
            }
        }
        false
    }
}
