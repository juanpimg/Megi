//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;

/// The normative default HTML5 User-Agent stylesheet string.
pub const USER_AGENT_STYLESHEET: &str = r#"
/* WHATWG HTML §15.3.1 Display types */
html, body, div, p, blockquote, center, figure, figcaption, header, footer,
nav, section, article, aside, main, h1, h2, h3, h4, h5, h6,
ul, ol, dl, dt, dd, pre, address, hr, fieldset, form {
    display: block;
}


center {
    text-align: center;
}


li {
    display: list-item;
}


/* Hidden elements (§15.3.2) */
head, link, meta, script, style, template, title {
    display: none;
}


/* dir attribute (§15.3) */
[dir="rtl"] {
    direction: rtl;
}

[dir="ltr"] {
    direction: ltr;
}


/* [hidden] attribute (§14.3) */
[hidden] {
    display: none !important;
}


/* Dialog (§15.3.2 / HTMLDialogElement): a closed dialog is not rendered. */
dialog:not([open]) {
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
    text-align: left;
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


/* ---------------------------------------------------------------------------------------
 * Form controls (approximate desktop UA styling; no native widgets).
 * The shell marks the focused control with `data-megi-focused` and a pressed button with
 * `data-megi-active`; `checked`/`disabled`/`readonly` are plain DOM attributes, while
 * `:checked`/`:disabled`/`:focus` are dynamic pseudo-classes this engine does not track.
 * ------------------------------------------------------------------------------------- */
form {
    display: block;
}

fieldset {
    display: block;
    margin-block: 0 0.75em;
    margin-inline: 2px;
    padding-block: 0.35em 0.625em;
    padding-inline: 0.75em;
    border: 1px solid #c0c0c0;
}

legend {
    display: block;
    padding-inline: 2px;
    font-weight: bold;
}

/* WHATWG HTML §15.3.2: unvisited links get the UA link colour and an underline. */
a:link {
    color: #0000ee;
    text-decoration: underline;
}

label {
    display: inline;
}


input, textarea, select, button {
    display: inline-block;
    box-sizing: border-box;
    vertical-align: middle;
    padding: 1px 2px;
    border: 1px solid #767676;
    background-color: #ffffff;
    color: #000000;
    max-width: 100%;
}

input[type=text], input[type=search], input[type=email], input[type=password],
input[type=number], input[type=file], input[type=date] {
    width: 10em;
    min-width: 5em;
    height: 1.6em;
}

textarea {
    display: inline-block;
    width: 20em;
    min-width: 8em;
    height: 3.4em;
    padding: 2px;
    vertical-align: top;
    overflow: hidden;
}

select {
    display: inline-block;
    width: 8em;
    height: 1.6em;
    padding: 0 2px;
    overflow: hidden;
}

option {
    display: block;
    padding: 0 2px;
}

input[type=submit], input[type=button], input[type=reset], button {
    display: inline-block;
    width: auto;
    min-width: 5em;
    height: 1.7em;
    padding: 1px 6px;
    background-color: #efefef;
    text-align: center;
}

input[type=checkbox], input[type=radio] {
    display: inline-block;
    width: 13px;
    height: 13px;
    min-width: 0;
    padding: 0;
    margin: 3px 3px 3px 4px;
    background-color: #ffffff;
    vertical-align: middle;
}

input[type=radio] {
    border-radius: 50%;
}

input[type=checkbox][checked] {
    background-color: #4d4d4d;
}

input[type=radio][checked] {
    background-color: #4d4d4d;
}

input[disabled], textarea[disabled], select[disabled], button[disabled],
input[disabled][checked] {
    background-color: #f2f2f2;
    color: #808080;
    border-color: #c0c0c0;
}

input[readonly], textarea[readonly] {
    background-color: #f7f7f7;
}

input[data-megi-focused], textarea[data-megi-focused], select[data-megi-focused],
button[data-megi-focused] {
    border-color: #4d90fe;
}

a:focus, a[data-megi-focused] {
    background-color: #cce0ff;
}

input[type=submit][data-megi-active], input[type=button][data-megi-active],
input[type=reset][data-megi-active], button[data-megi-active] {
    background-color: #d7d7d7;
    border-color: #4d90fe;
}


/* HTML <dialog> & top layer (WHATWG §15.3) */
dialog {
    display: block;
    position: absolute;
    left: 0;
    right: 0;
    width: fit-content;
    height: fit-content;
    margin: auto;
    border: 2px solid #000000;
    padding: 1em;
    background-color: #ffffff;
    color: #000000;
}

dialog:not([open]) {
    display: none;
}

dialog:modal {
    position: fixed;
    top: 0;
    bottom: 0;
    max-width: calc(100% - 6px - 2em);
    max-height: calc(100% - 6px - 2em);
    overflow: auto;
}

dialog:modal::backdrop {
    background-color: rgba(0, 0, 0, 0.1);
}

"#;

/// Parses the default User-Agent stylesheet into a [`Stylesheet`].
pub fn parse_user_agent_stylesheet() -> Stylesheet {
    crate::parser::parse_stylesheet(USER_AGENT_STYLESHEET)
}

/// Creates a [`StyleCollection`] pre-loaded with the default HTML5 User-Agent stylesheet.
/// Parsed User-Agent stylesheet cached for the lifetime of the process.
///
/// The sheet is immutable: caching the **AST** avoids re-tokenizing/parsing the same
/// UA CSS on every style-resolution pass while still compiling selectors into the
/// per-resolution collection (which stays cheap and avoids cloning compiled rules).
pub(crate) fn cached_user_agent_stylesheet() -> &'static crate::rule::Stylesheet {
    static CACHE: OnceLock<crate::rule::Stylesheet> = OnceLock::new();
    CACHE.get_or_init(parse_user_agent_stylesheet)
}

pub fn create_user_agent_style_collection() -> StyleCollection {
    let mut collection = StyleCollection::new();
    collection.add_stylesheet(cached_user_agent_stylesheet(), Origin::UserAgent);
    collection
}
