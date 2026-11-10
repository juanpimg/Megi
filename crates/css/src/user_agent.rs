//! # WHATWG HTML §15 Default User-Agent Stylesheet
//!
//! Provides the normative default UA styles for HTML elements per
//! <https://html.spec.whatwg.org/multipage/rendering.html>.

use crate::cascade::{Origin, StyleCollection};
use crate::rule::Stylesheet;

/// The normative default HTML5 User-Agent stylesheet string.
pub const USER_AGENT_STYLESHEET: &str = r#"
/* WHATWG HTML §15.3.1 Display types */
html, body, div, p, blockquote, figure, figcaption, header, footer,
nav, section, article, aside, main, h1, h2, h3, h4, h5, h6,
ul, ol, dl, dt, dd, pre, address, hr, fieldset, form {
    display: block;
}

li {
    display: list-item;
}

/* Hidden elements (§15.3.2) */
head, link, meta, script, style, template, title {
    display: none;
}

/* Page margin (§15.3.3) */
body {
    margin: 8px;
}

/* Headings margin and font-size (§15.3.6) */
h1 {
    font-size: 2em;
    margin-block: 0.67em;
    font-weight: bold;
}
h2 {
    font-size: 1.5em;
    margin-block: 0.83em;
    font-weight: bold;
}
h3 {
    font-size: 1.17em;
    margin-block: 1em;
    font-weight: bold;
}
h4 {
    font-size: 1em;
    margin-block: 1.33em;
    font-weight: bold;
}
h5 {
    font-size: 0.83em;
    margin-block: 1.67em;
    font-weight: bold;
}
h6 {
    font-size: 0.67em;
    margin-block: 2.33em;
    font-weight: bold;
}

/* Paragraphs, preformatted text & quotes */
p {
    margin-block: 1em;
}
pre {
    white-space: pre;
    font-family: monospace;
    margin-block: 1em;
}
blockquote {
    margin-block: 1em;
    margin-inline: 40px;
}
ul, ol {
    margin-block: 1em;
    padding-inline: 40px;
}
ul {
    list-style-type: disc;
}
ol {
    list-style-type: decimal;
}

/* Text-level semantics (§15.3.7) */
b, strong {
    font-weight: bold;
}
i, em, cite, var, dfn {
    font-style: italic;
}
code, kbd, samp {
    font-family: monospace;
}
small {
    font-size: smaller;
}
hr {
    margin-block: 0.5em;
    border-style: solid;
    border-width: 1px;
}

/* Tables (§15.3.8) */
table {
    display: table;
    border-collapse: separate;
    border-spacing: 2px;
    border-color: gray;
    box-sizing: border-box;
}
caption {
    display: table-caption;
    text-align: center;
}
thead {
    display: table-header-group;
}
tbody {
    display: table-row-group;
}
tfoot {
    display: table-footer-group;
}
tr {
    display: table-row;
}
colgroup {
    display: table-column-group;
}
col {
    display: table-column;
}
td, th {
    display: table-cell;
    padding: 1px;
}
th {
    font-weight: bold;
    text-align: center;
}
"#;

/// Parses the default User-Agent stylesheet into a [`Stylesheet`].
pub fn parse_user_agent_stylesheet() -> Stylesheet {
    crate::parser::parse_stylesheet(USER_AGENT_STYLESHEET)
}

/// Creates a [`StyleCollection`] pre-loaded with the default HTML5 User-Agent stylesheet.
pub fn create_user_agent_style_collection() -> StyleCollection {
    let mut collection = StyleCollection::new();
    let ua_sheet = parse_user_agent_stylesheet();
    collection.add_stylesheet(&ua_sheet, Origin::UserAgent);
    collection
}
