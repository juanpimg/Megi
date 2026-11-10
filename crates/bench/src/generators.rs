//! Deterministic synthetic workload generators.
//!
//! Every generator is a pure function of its size argument: no randomness, no
//! clock, no environment. Two runs with the same `--scale` therefore parse the
//! exact same byte stream, which makes the benchmark numbers comparable across
//! machines and commits.
//!
//! The sizes are intentionally generous: the benchmark measures *engine* cost,
//! not fixture construction. Callers build the fixtures once (outside the timed
//! loop) and scale them through [`scale`].

use std::fmt::Write as _;

/// Fixed lorem-ipsum sentence used by the realistic document generator.
const LOREM: &str = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod \
tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud \
exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. ";

/// Fixed vocabulary used by the font benchmarks (deterministic, ASCII only).
#[cfg(feature = "fonts")]
const WORDS: [&str; 12] = [
    "the",
    "quick",
    "brown",
    "fox",
    "jumps",
    "over",
    "lazy",
    "dog",
    "megi",
    "renders",
    "typography",
    "benchmark",
];

/// Applies the `--scale` factor to a base count, never returning zero.
pub fn scale(base: usize, factor: f64) -> usize {
    let scaled = (base as f64 * factor).round();
    if scaled < 1.0 { 1 } else { scaled as usize }
}

/// A generated "realistic" page plus the statistics reported in the table.
pub struct RealisticDocument {
    pub html: String,
    pub sections: usize,
    pub style_rules: usize,
}

/// Generates a realistic multi-section article page of at least `target_bytes`.
///
/// The page mixes headings, paragraphs, inline formatting, links, unordered
/// lists and tables, and embeds an author stylesheet (CSS rules) so downstream
/// style resolution and layout have non-trivial input.
pub fn realistic_document(target_bytes: usize) -> RealisticDocument {
    let mut html = String::with_capacity(target_bytes + target_bytes / 2);
    html.push_str(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <title>Megi benchmark document</title>\n<style>\n",
    );
    let mut style_rules = 0usize;
    for i in 0..128usize {
        let _ = writeln!(
            html,
            ".card-{i} {{ color: #{:02x}{:02x}{:02x}; padding: {}px; margin: {}px; }}",
            i % 251,
            (i * 7) % 251,
            (i * 13) % 251,
            2 + (i % 11),
            1 + (i % 7)
        );
        style_rules += 1;
    }
    for tag in ["body", "h1", "h2", "p", "li", "td", "a"] {
        let _ = writeln!(html, "{tag} {{ line-height: 1.4; }}");
        style_rules += 1;
    }
    html.push_str("</style>\n</head>\n<body>\n<div id=\"app\" class=\"app\">\n");

    let mut sections = 0usize;
    while html.len() < target_bytes {
        let i = sections;
        let _ = writeln!(
            html,
            "<section id=\"section-{i}\" class=\"card-{}\">",
            i % 128
        );
        let _ = writeln!(html, "<h2>Section {i}</h2>");
        let _ = writeln!(html, "<p>{LOREM}{LOREM}</p>");
        let _ = writeln!(
            html,
            "<p>Mixed <b>bold</b>, <i>italic</i> and <a href=\"/page/{i}\">link {i}</a> content.</p>"
        );
        html.push_str("<ul>\n");
        for item in 0..5usize {
            let _ = writeln!(html, "<li>Item {item} of section {i}</li>");
        }
        html.push_str("</ul>\n<table>\n");
        for row in 0..3usize {
            html.push_str("<tr>");
            for cell in 0..3usize {
                let _ = write!(html, "<td>r{row}c{cell}-{i}</td>");
            }
            html.push_str("</tr>\n");
        }
        html.push_str("</table>\n</section>\n");
        sections += 1;
    }
    html.push_str("</div>\n</body>\n</html>\n");
    RealisticDocument {
        html,
        sections,
        style_rules,
    }
}

/// `<div>` nested `depth` levels deep, with a single text node at the bottom.
pub fn nested_divs(depth: usize) -> String {
    let mut html = String::with_capacity(depth.saturating_mul(11).saturating_add(64));
    html.push_str("<!DOCTYPE html><html><body>");
    for _ in 0..depth {
        html.push_str("<div>");
    }
    html.push('x');
    for _ in 0..depth {
        html.push_str("</div>");
    }
    html.push_str("</body></html>");
    html
}

/// One paragraph followed by `count` stray `</p>` end tags.
pub fn massive_end_p(count: usize) -> String {
    let mut html = String::with_capacity(count.saturating_mul(4).saturating_add(64));
    html.push_str("<!DOCTYPE html><html><body><p>text");
    for _ in 0..count {
        html.push_str("</p>");
    }
    html.push_str("</body></html>");
    html
}

/// `count` misnested `<b><i><u>` blocks; every close exercises the adoption agency.
pub fn massive_formatting(count: usize) -> String {
    let mut html = String::with_capacity(count.saturating_mul(28).saturating_add(64));
    html.push_str("<!DOCTYPE html><html><body>");
    for _ in 0..count {
        html.push_str("<b><i><u>text</b></i></u>");
    }
    html.push_str("</body></html>");
    html
}

/// A single element carrying `count` distinct attributes.
pub fn tag_with_attributes(count: usize) -> String {
    let mut html = String::with_capacity(count.saturating_mul(20).saturating_add(64));
    html.push_str("<!DOCTYPE html><html><body><div");
    for i in 0..count {
        let _ = write!(html, " a{i}=\"v{i}\"");
    }
    html.push_str(">x</div></body></html>");
    html
}

/// A pretty-printed (heavily indented) document with `blocks` repeated blocks.
pub fn pretty_printed(blocks: usize) -> String {
    let mut html = String::with_capacity(blocks.saturating_mul(150).saturating_add(256));
    html.push_str(
        "<!DOCTYPE html>\n<html>\n  <head>\n    <title>Pretty</title>\n  </head>\n  <body>\n",
    );
    for i in 0..blocks {
        let _ = writeln!(
            html,
            "    <div class=\"block\" id=\"block-{i}\">\n      \
             <h3>Block {i}</h3>\n      <p>Some text for block {i}.</p>\n    </div>"
        );
    }
    html.push_str("  </body>\n</html>\n");
    html
}

/// A stylesheet with `rules` distinct class rules plus a few base rules.
pub fn stylesheet_rules(rules: usize) -> String {
    let mut css = String::with_capacity(rules.saturating_mul(80).saturating_add(128));
    for i in 0..rules {
        let _ = writeln!(
            css,
            ".rule-{i} {{ color: #{:02x}00{:02x}; padding: {}px; margin: {}px {}px; }}",
            i % 256,
            (i * 37) % 256,
            i % 12,
            i % 9,
            (i * 3) % 17
        );
    }
    css.push_str("body { font-size: 16px; }\np { line-height: 1.4; }\n");
    css
}

/// A flat document with approximately `target_elements` elements.
///
/// Layout: `<div class="item cN" id="eI"><span class="label sM">text</span></div>`
/// repeated inside a single `#app` container. Two elements per group, plus the
/// `html`/`body`/`#app` wrappers.
pub fn element_document(target_elements: usize) -> String {
    let groups = target_elements.saturating_div(2).max(1);
    let mut html = String::with_capacity(groups.saturating_mul(120).saturating_add(256));
    html.push_str(
        "<!DOCTYPE html><html><head><title>Elements</title></head><body><div id=\"app\" \
         class=\"app\">",
    );
    for i in 0..groups {
        let _ = write!(
            html,
            "<div class=\"item c{}\" id=\"e{i}\"><span class=\"label s{}\">text {i}</span></div>",
            i % 64,
            i % 16
        );
    }
    html.push_str("</div></body></html>");
    html
}

/// Author stylesheet matching the elements produced by [`element_document`].
pub fn element_author_css() -> String {
    let mut css = String::with_capacity(4096);
    css.push_str("body { font-size: 16px; }\n#app { padding: 4px; }\n");
    for i in 0..64usize {
        let _ = writeln!(css, ".c{i} {{ color: #{:02x}1020; }}", i * 3);
    }
    for i in 0..16usize {
        let _ = writeln!(css, "span.s{i} {{ padding: {}px; }}", i % 5);
    }
    for i in 0..32usize {
        let _ = writeln!(css, "div.c{i} span.label {{ margin: {}px; }}", i % 7);
    }
    for i in 0..16usize {
        let _ = writeln!(css, "div.item.c{i} {{ border-width: {}px; }}", i % 3);
    }
    css
}

/// Deterministic list of `count` words for text measurement / shaping.
#[cfg(feature = "fonts")]
pub fn words(count: usize) -> Vec<String> {
    (0..count)
        .map(|i| format!("{}{}", WORDS[i % WORDS.len()], i % 97))
        .collect()
}

// ---------------------------------------------------------------------------
// Default workload sizes (before `--scale` is applied).
// ---------------------------------------------------------------------------

/// Target size of the "realistic" HTML page (~1 MiB).
pub const REALISTIC_TARGET_BYTES: usize = 1_048_576;
/// Nesting depths measured by the adversarial `<div>` benchmark.
pub const NESTED_DEPTHS: [usize; 3] = [10_000, 32_000, 64_000];
/// Number of trailing `</p>` tags in the stray-end-tag benchmark.
pub const END_P_TAGS: usize = 100_000;
/// Number of misnested `<b><i><u>` blocks in the formatting benchmark.
pub const FORMATTING_BLOCKS: usize = 10_000;
/// Number of attributes attached to a single element.
pub const ATTRIBUTE_COUNT: usize = 40_000;
/// Number of pretty-printed blocks in the formatting-preserving benchmark.
pub const PRETTY_BLOCKS: usize = 5_000;
/// Number of rules in the CSS parse benchmark.
pub const CSS_RULES: usize = 10_000;
/// Element counts used by the style-resolution benchmarks.
pub const STYLE_NODES: [usize; 2] = [2_000, 10_000];
/// Element count used by the selector-matching benchmarks.
pub const SELECTOR_NODES: usize = 10_000;
/// Word count used by the optional font benchmarks.
#[cfg(feature = "fonts")]
pub const FONT_WORDS: usize = 1_000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaling_never_returns_zero() {
        assert_eq!(scale(0, 1.0), 1);
        assert_eq!(scale(10, 0.01), 1);
        assert_eq!(scale(10_000, 0.5), 5_000);
        assert_eq!(scale(10_000, 2.0), 20_000);
    }

    #[test]
    fn generators_are_deterministic() {
        assert_eq!(nested_divs(64), nested_divs(64));
        assert_eq!(massive_end_p(32), massive_end_p(32));
        assert_eq!(massive_formatting(8), massive_formatting(8));
        assert_eq!(tag_with_attributes(16), tag_with_attributes(16));
        assert_eq!(pretty_printed(4), pretty_printed(4));
        assert_eq!(stylesheet_rules(8), stylesheet_rules(8));
        assert_eq!(element_document(8), element_document(8));
    }

    #[test]
    fn workloads_have_the_requested_shape() {
        let nested = nested_divs(100);
        assert_eq!(nested.matches("<div>").count(), 100);
        assert_eq!(nested.matches("</div>").count(), 100);
        assert!(nested.contains(">x<"));

        let ends = massive_end_p(50);
        assert_eq!(ends.matches("</p>").count(), 50);

        let attrs = tag_with_attributes(40);
        assert_eq!(attrs.matches("=\"").count(), 40);

        let pretty = pretty_printed(7);
        assert_eq!(pretty.matches("class=\"block\"").count(), 7);

        let rules = stylesheet_rules(11);
        assert_eq!(rules.matches(".rule-").count(), 11);
    }

    #[test]
    fn realistic_document_reaches_the_target_size() {
        let document = realistic_document(64 * 1024);
        assert!(document.html.len() >= 64 * 1024);
        assert!(document.sections > 0);
        assert!(document.style_rules >= 128);
        assert!(document.html.starts_with("<!DOCTYPE html>"));
        assert!(document.html.ends_with("</html>\n"));
    }
}
