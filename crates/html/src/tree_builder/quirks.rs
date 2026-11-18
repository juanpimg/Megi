//! Doctype quirks detection and the "special" category (§13.2.4.2).

use megi_core::{Namespace, QuirksMode};

use super::*;

pub(crate) fn determine_doctype_quirks_mode(
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
