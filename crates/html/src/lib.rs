//! # Megi HTML (`html`)
//!
//! Clean-room Rust implementation of the WHATWG HTML Living Standard (§13.2 Parsing HTML
//! documents, §13.3 Serializing HTML fragments, and §13.4 Parsing HTML fragments).

#![forbid(unsafe_code)]

pub mod entities;
pub mod input_stream;
pub mod serializer;
pub mod tokenizer;
pub mod tree_builder;

pub use dom::{Document, Namespace};
pub use serializer::{
    HtmlSerializeOptions, serialize_dom_for_test, serialize_fragment,
    serialize_fragment_with_options, serialize_html, serialize_html_with_options,
    serialize_outer_html, serialize_outer_html_with_options,
};
pub use tokenizer::{
    ParseError, ParseErrorCode, Token, Tokenizer, TokenizerConfig, TokenizerState,
};
pub use tree_builder::{
    InsertionMode, LimitViolations, ParseOptions, ParseOutcome, TreeBuilder, parse_document,
    parse_document_with_options, parse_fragment, parse_fragment_with_options,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_document_and_serialize_canonical_tree() {
        let doc = parse_document("<!DOCTYPE html><p>Hello <b>world</b>!</p>", false);
        let dump = serialize_dom_for_test(&doc);
        let expected = "\
| <!DOCTYPE html>
| <html>
|   <head>
|   <body>
|     <p>
|       \"Hello \"
|       <b>
|         \"world\"
|       \"!\"";
        assert_eq!(dump, expected);
    }

    #[test]
    fn test_foster_parenting_places_misnested_element_before_table() {
        let doc = parse_document(
            "<table><b>Fostered</b><tr><td>Cell</td></tr></table>",
            false,
        );
        let dump = serialize_dom_for_test(&doc);
        let expected = "\
| <html>
|   <head>
|   <body>
|     <b>
|       \"Fostered\"
|     <table>
|       <tbody>
|         <tr>
|           <td>
|             \"Cell\"";
        assert_eq!(dump, expected);
    }

    #[test]
    fn test_template_contents_serialization() {
        let doc = parse_document(
            "<body><template><span>Inert</span></template></body>",
            false,
        );
        let dump = serialize_dom_for_test(&doc);
        let expected = "\
| <html>
|   <head>
|   <body>
|     <template>
|       content
|         <span>
|           \"Inert\"";
        assert_eq!(dump, expected);
    }
}
