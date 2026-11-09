//! # WHATWG HTML §13.2.6 Tree Construction & §13.4 Fragment Parsing
//!
//! Implements the 21 Insertion Modes of the modern WHATWG HTML Living Standard
//! (§13.2.6.4.1 through §13.2.6.4.21), the Stack of Open Elements with all 5 scope
//! checks (§13.2.4.2), the List of Active Formatting Elements (§13.2.4.3), Foster
//! Parenting (§13.2.6.1), the Adoption Agency Algorithm (§13.2.6.4.7), `<template>`
//! contents insertion, and Foreign Content (SVG/MathML) attribute/tag adjustments (§13.2.6.5).

use dom::{Attribute, Document, Namespace, NodeId, NodeKind, QuirksMode};

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

    if let Some(sys) = system_id {
        if sys.eq_ignore_ascii_case("http://www.ibm.com/data/dtd/v11/ibmxhtml1-transitional.dtd") {
            return QuirksMode::Quirks;
        }
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
}

impl TreeBuilder {
    pub fn new(scripting_enabled: bool) -> Self {
        Self {
            doc: Document::new(),
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
        if let Some(id) = self.adjusted_current_node() {
            if let Some(el) = self.doc.node(id).as_element() {
                return el.namespace != Namespace::Html;
            }
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

    pub fn should_dispatch_to_foreign(&self, token: &Token) -> bool {
        if self.open_elements.is_empty() {
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

        if el.namespace == Namespace::MathMl && el.local_name == "annotation-xml" {
            if let Token::StartTag { name, .. } = token {
                if name == "svg" {
                    return false;
                }
            }
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
        for &node_id in self.open_elements.iter().rev() {
            if node_id == target_node {
                return true;
            }
            if let Some(el) = self.doc.node(node_id).as_element() {
                if DEFAULT_SCOPE_ELEMENTS
                    .iter()
                    .any(|&(s_name, s_ns)| el.namespace == s_ns && el.local_name == s_name)
                {
                    return false;
                }
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
                self.open_elements.pop();
            } else {
                break;
            }
        }
    }

    pub fn close_p_element(&mut self) {
        self.generate_implied_end_tags(Some("p"));
        while let Some(node_id) = self.open_elements.pop() {
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

    fn insert_node_at_adjusted_location(&mut self, node_id: NodeId) {
        let loc = self.appropriate_place_for_inserting_node(None);
        if let Some(sib) = loc.insert_before_sibling {
            self.doc.insert_before(sib, node_id);
        } else {
            self.doc.append_child(loc.parent, node_id);
        }
    }

    fn insert_element(
        &mut self,
        namespace: Namespace,
        name: &str,
        raw_attrs: &[(String, String)],
    ) -> NodeId {
        let attrs: Vec<Attribute> = raw_attrs
            .iter()
            .map(|(k, v)| Attribute::new(k.clone(), v.clone()))
            .collect();
        let el_id = self.doc.create_element(namespace, name, attrs);
        self.insert_node_at_adjusted_location(el_id);
        self.open_elements.push(el_id);
        el_id
    }

    fn insert_html_element(&mut self, name: &str, raw_attrs: &[(String, String)]) -> NodeId {
        self.insert_element(Namespace::Html, name, raw_attrs)
    }

    fn create_html_element(&mut self, name: &str, raw_attrs: &[(String, String)]) -> NodeId {
        let attrs: Vec<Attribute> = raw_attrs
            .iter()
            .map(|(k, v)| Attribute::new(k.clone(), v.clone()))
            .collect();
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
        self.open_elements.push(el_id);
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

    fn reconstruct_active_formatting_elements(&mut self) {
        if self.active_formatting.is_empty() {
            return;
        }
        match self.active_formatting.last() {
            None | Some(FormattingEntry::Marker) => return,
            Some(FormattingEntry::Element { node_id, .. }) => {
                if self.open_elements.contains(node_id) {
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
                    if self.open_elements.contains(node_id) {
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
        // Noah's Ark clause: max 3 identical elements after the last marker
        let mut matching_indices = Vec::new();
        for (idx, entry) in self.active_formatting.iter().enumerate().rev() {
            match entry {
                FormattingEntry::Marker => break,
                FormattingEntry::Element {
                    tag_name,
                    attributes,
                    ..
                } => {
                    if tag_name == name && attributes == attrs {
                        matching_indices.push(idx);
                    }
                }
            }
        }
        if matching_indices.len() >= 3
            && let Some(&earliest_idx) = matching_indices.last()
        {
            self.active_formatting.remove(earliest_idx);
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
        for idx in (0..self.open_elements.len()).rev() {
            let node_id = self.open_elements[idx];
            if self.node_in_html(node_id, name) {
                self.generate_implied_end_tags(Some(name));
                while self.open_elements.len() > idx {
                    self.open_elements.pop();
                }
                break;
            }
            if self.is_node_special(node_id) {
                break;
            }
        }
    }

    fn run_adoption_agency_algorithm(&mut self, subject: &str) {
        if let Some(curr) = self.current_node() {
            if self.node_in_html(curr, subject) {
                let in_active = self.active_formatting.iter().any(
                    |e| matches!(e, FormattingEntry::Element { node_id, .. } if *node_id == curr),
                );
                if !in_active {
                    self.open_elements.pop();
                    return;
                }
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
                while let Some(popped) = self.open_elements.pop() {
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
            let common_ancestor_id = self.open_elements[fmt_stack_pos - 1];
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
                if inner_loop_counter > 3 {
                    if let Some(idx) = node_fmt_idx {
                        self.active_formatting.remove(idx);
                        if idx <= bookmark && bookmark > 0 {
                            bookmark -= 1;
                        }
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
                        self.open_elements.remove(pos);
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
                self.open_elements[stack_pos] = new_node;
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
                self.open_elements.remove(old_stack_pos);
            }
            let fb_stack_pos = self
                .open_elements
                .iter()
                .rposition(|&id| id == furthest_block_id)
                .expect("furthest_block must be in open_elements");
            self.open_elements.insert(fb_stack_pos + 1, new_element);
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
            self.open_elements.pop();
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
            self.open_elements.pop();
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
            self.open_elements.pop();
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
                    self.open_elements.pop();
                }
                self.process_token_html(token)
            }
            Token::EndTag { ref name, .. } if matches!(name.as_str(), "br" | "p") => {
                while let Some(curr) = self.current_node() {
                    if self.is_integration_point_or_html(curr) {
                        break;
                    }
                    self.open_elements.pop();
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
                    self.open_elements.pop();
                }
                None
            }
            Token::EndTag { ref name, .. }
                if name == "script"
                    && self
                        .current_node()
                        .is_some_and(|id| self.node_is(id, Namespace::Svg, "script")) =>
            {
                self.open_elements.pop();
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
                            self.open_elements.pop();
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
                    let html_id = self.insert_html_element("html", attributes);
                    let _ = html_id;
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
                    self.open_elements.pop();
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
                    self.open_elements.pop();
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
                    self.open_elements.pop();
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
                        self.open_elements.push(head_id);
                        let res = self.process_in_head(token);
                        if let Some(pos) = self.open_elements.iter().rposition(|&id| id == head_id)
                        {
                            self.open_elements.remove(pos);
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
                Token::Eof => {
                    self.open_elements.pop();
                    self.mode = self.original_mode;
                    self.process_token(Token::Eof)
                }
                Token::EndTag { .. } => {
                    self.open_elements.pop();
                    self.mode = self.original_mode;
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
                    if !self
                        .open_elements
                        .iter()
                        .any(|&id| self.node_in_html(id, "template"))
                    {
                        return None;
                    }
                    while let Some(id) = self.open_elements.pop() {
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
                    self.open_elements.pop();
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
                    self.open_elements.pop();
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
                self.open_elements.pop();
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
                self.open_elements.pop();
                self.mode = InsertionMode::AfterHead;
                None
            }
            Token::EndTag { ref name, .. } if matches!(name.as_str(), "body" | "html" | "br") => {
                self.open_elements.pop();
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
                if !self
                    .open_elements
                    .iter()
                    .any(|&id| self.node_in_html(id, "template"))
                {
                    return None;
                }
                self.generate_implied_end_tags(None);
                while let Some(id) = self.open_elements.pop() {
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
                self.open_elements.pop();
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
                if !self
                    .open_elements
                    .iter()
                    .any(|&id| self.node_in_html(id, "template"))
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
                if self.open_elements.len() >= 2
                    && !self
                        .open_elements
                        .iter()
                        .any(|&id| self.node_in_html(id, "template"))
                {
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
                    self.open_elements.pop();
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
                        while let Some(popped) = self.open_elements.pop() {
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
                        while let Some(popped) = self.open_elements.pop() {
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
                    self.open_elements.pop();
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
                if let Some((_, ref ctx_name)) = self.fragment_context {
                    if ctx_name == "select" {
                        return None;
                    }
                }
                if self.has_element_in_scope("select") {
                    while let Some(popped) = self.open_elements.pop() {
                        if self.node_in_html(popped, "select") {
                            break;
                        }
                    }
                }
                self.reconstruct_active_formatting_elements();
                self.insert_html_element("input", attributes);
                self.open_elements.pop();
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
                self.open_elements.pop();
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
                let parsing_template = !self.template_modes.is_empty()
                    || self
                        .open_elements
                        .iter()
                        .any(|&id| self.node_in_html(id, "template"));
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
                    while let Some(popped) = self.open_elements.pop() {
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
                        self.open_elements.remove(pos);
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
                    self.open_elements.pop();
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
                if let Some((_, ref ctx_name)) = self.fragment_context {
                    if ctx_name == "select" {
                        return None;
                    }
                }
                if self.has_element_in_scope("select") {
                    while let Some(popped) = self.open_elements.pop() {
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
                } else if let Some(curr) = self.current_node() {
                    if self.node_in_html(curr, "option") {
                        self.open_elements.pop();
                    }
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
                } else if let Some(curr) = self.current_node() {
                    if self.node_in_html(curr, "option") {
                        self.open_elements.pop();
                    }
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
                    while let Some(popped) = self.open_elements.pop() {
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
                    while let Some(popped) = self.open_elements.pop() {
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
                    while let Some(popped) = self.open_elements.pop() {
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
                let parsing_template = !self.template_modes.is_empty()
                    || self
                        .open_elements
                        .iter()
                        .any(|&id| self.node_in_html(id, "template"));
                if !parsing_template {
                    let node = self.form_element.take();
                    let node_id = node?;
                    if !self.has_node_in_scope(node_id) {
                        return None;
                    }
                    self.generate_implied_end_tags(None);
                    if let Some(pos) = self.open_elements.iter().rposition(|&id| id == node_id) {
                        self.open_elements.remove(pos);
                    }
                } else {
                    if !self.has_element_in_scope("form") {
                        return None;
                    }
                    self.generate_implied_end_tags(None);
                    while let Some(popped) = self.open_elements.pop() {
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
                while let Some(popped) = self.open_elements.pop() {
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
                while let Some(popped) = self.open_elements.pop() {
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
                self.open_elements.pop();
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
                    while let Some(popped) = self.open_elements.pop() {
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
                    while let Some(popped) = self.open_elements.pop() {
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
                let parsing_template = self
                    .open_elements
                    .iter()
                    .any(|&id| self.node_in_html(id, "template"));
                if self.form_element.is_some() && !parsing_template {
                    return None;
                }
                let form_id = self.insert_html_element("form", attributes);
                if !parsing_template {
                    self.form_element = Some(form_id);
                }
                self.open_elements.pop();
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
                self.open_elements.pop();
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
                while let Some(popped) = self.open_elements.pop() {
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
                while let Some(popped) = self.open_elements.pop() {
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
                while let Some(popped) = self.open_elements.pop() {
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
                self.open_elements.pop();
                None
            }
            Token::EndTag { ref name, .. } if name == "colgroup" => {
                if self
                    .current_node()
                    .is_some_and(|id| self.node_in_html(id, "colgroup"))
                {
                    self.open_elements.pop();
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
                    self.open_elements.pop();
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
                    self.open_elements.pop();
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
                self.open_elements.pop();
                self.mode = InsertionMode::InTable;
                self.process_token(token)
            }
            Token::EndTag { ref name, .. } if name == "table" => {
                if !self.has_table_body_in_table_scope() {
                    return None;
                }
                self.clear_stack_back_to_table_body_context();
                self.open_elements.pop();
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
                    self.open_elements.pop();
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
                    self.open_elements.pop();
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
                    self.open_elements.pop();
                    self.mode = InsertionMode::InTableBody;
                    return self.process_token(token);
                }
                None
            }
            Token::EndTag { ref name, .. } if name == "table" => {
                if self.has_element_in_table_scope("tr") {
                    self.clear_stack_back_to_table_row_context();
                    self.open_elements.pop();
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
        while let Some(popped) = self.open_elements.pop() {
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

    fn collect_descendants_within_select(&self, parent: NodeId, name: &str, out: &mut Vec<NodeId>) {
        let mut curr = self.doc.node(parent).first_child;
        while let Some(child_id) = curr {
            let next = self.doc.node(child_id).next_sibling;
            if self.node_in_html(child_id, name) {
                out.push(child_id);
            }
            if !self.node_in_html(child_id, "select") {
                self.collect_descendants_within_select(child_id, name, out);
            }
            curr = next;
        }
    }

    pub fn update_selectedcontent_elements(&mut self) {
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

/// Parses a complete HTML document string into a [`dom::Document`] (§13.2).
pub fn parse_document(input: &str, scripting_enabled: bool) -> Document {
    let mut tok = Tokenizer::from_str_input(input, TokenizerConfig::default());
    let mut tb = TreeBuilder::new(scripting_enabled);

    loop {
        tok.allow_cdata = tb.is_adjusted_current_node_foreign();
        let prev_len = tok.tokens.len();
        tok.step();
        if tok.tokens.len() > prev_len {
            let emitted: Vec<Token> = tok.tokens.drain(prev_len..).collect();
            let mut saw_eof = false;
            for token in emitted {
                if token == Token::Eof {
                    saw_eof = true;
                }
                if let Some(new_state) = tb.process_token(token) {
                    tok.state = new_state;
                }
                tok.allow_cdata = tb.is_adjusted_current_node_foreign();
            }
            if saw_eof {
                break;
            }
        }
    }

    // Ensure `<html>`, `<head>`, and `<body>` exist if parsing ended before `InBody`
    let root = tb.doc.root();
    let has_html = tb.doc.children(root).any(|id| tb.node_in_html(id, "html"));
    if !has_html && tb.mode != InsertionMode::InFrameset && tb.mode != InsertionMode::AfterFrameset
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
    tb.doc
}

/// Parses an HTML fragment in the context of `(context_namespace, context_local_name)` (§13.4)
/// and returns a [`dom::Document`] whose root contains the resulting fragment children.
pub fn parse_fragment(
    input: &str,
    context_namespace: Namespace,
    context_local_name: &str,
    scripting_enabled: bool,
) -> Document {
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

    let mut tb = TreeBuilder::new(scripting_enabled);
    tb.fragment_context = Some((context_namespace, context_local_name.to_owned()));
    let ctx_node = tb
        .doc
        .create_element(context_namespace, context_local_name, vec![]);
    tb.fragment_context_node = Some(ctx_node);

    let html_root = tb.doc.create_element(Namespace::Html, "html", vec![]);
    tb.doc.append_child(tb.doc.root(), html_root);
    tb.open_elements.push(html_root);

    if context_namespace == Namespace::Html && context_local_name == "template" {
        tb.template_modes.push(InsertionMode::InTemplate);
    }
    tb.reset_insertion_mode_appropriately();

    loop {
        tok.allow_cdata = tb.is_adjusted_current_node_foreign();
        let prev_len = tok.tokens.len();
        tok.step();
        if tok.tokens.len() > prev_len {
            let emitted: Vec<Token> = tok.tokens.drain(prev_len..).collect();
            let mut saw_eof = false;
            for token in emitted {
                if token == Token::Eof {
                    saw_eof = true;
                }
                if let Some(new_state) = tb.process_token(token) {
                    tok.state = new_state;
                }
                tok.allow_cdata = tb.is_adjusted_current_node_foreign();
            }
            if saw_eof {
                break;
            }
        }
    }

    // Move children of synthetic `<html>` directly under `doc.root()` for canonical fragment serialization
    let root = tb.doc.root();
    tb.doc.reparent_children(html_root, root);
    tb.doc.detach(html_root);
    if let Some(ctx_id) = tb.fragment_context_node {
        tb.doc.detach(ctx_id);
    }
    tb.update_selectedcontent_elements();
    tb.doc
}
