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

/// Deterministic HTML page with `boxes` boxes painted with CSS gradients.
///
/// Alternates `linear-gradient()` (angles, multi-stop, alpha) and
/// `radial-gradient()` (circles/ellipses at deterministic positions) so the
/// style parser, display-list painter and raster tiler exercise the CSS
/// gradient code paths end to end. No network, no timers, no randomness.
pub fn gradients_page(boxes: usize) -> String {
    let mut html = String::with_capacity(boxes.saturating_mul(260).saturating_add(512));
    html.push_str(
        "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\"><title>Gradients</title>\n<style>\n\
         html,body{margin:0;padding:0;background:#ffffff}\n\
         .wrap{display:flex;flex-wrap:wrap;width:800px}\n",
    );
    for i in 0..boxes {
        let a = (i * 37) % 360;
        let x = 10 + (i * 13) % 80;
        let y = 10 + (i * 29) % 80;
        let r = (i * 17) % 256;
        let g = (i * 43) % 256;
        let b = (i * 71) % 256;
        let a2 = (i * 91) % 256;
        let _ = writeln!(
            html,
            ".g{i}{{width:110px;height:70px;margin:4px;background-image:",
        );
        if i % 2 == 0 {
            let _ = writeln!(
                html,
                "linear-gradient({a}deg,#{r:02x}{g:02x}{b:02x} 0%,#{b:02x}{r:02x}{a2:02x} 40%,#{g:02x}{a2:02x}{r:02x} 100%);}}",
            );
        } else {
            let _ = writeln!(
                html,
                "radial-gradient(circle at {x}% {y}%,#{r:02x}{g:02x}{b:02x} 0%,#{b:02x}{a2:02x}{g:02x} 55%,#{a2:02x}{r:02x}{b:02x} 100%);}}",
            );
        }
    }
    html.push_str("</style></head><body><div class=\"wrap\">");
    for i in 0..boxes {
        let _ = write!(html, "<div class=\"g{i}\"></div>");
    }
    html.push_str("</div></body></html>\n");
    html
}

/// Number of text cells rendered by [`grid_page`] (3×3 + auto-fill + spans).
pub const GRID_CELLS: usize = 9 + 12 + 8;

/// Deterministic HTML page with three grids: fixed `1fr` columns, `auto-fill`
/// with `minmax()` tracks, and explicit row/column spans. Cells carry text so
/// layout has real content to measure.
pub fn grid_page() -> String {
    let mut html = String::with_capacity(8192);
    html.push_str(
        "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\"><title>Grids</title>\n<style>\n\
         html,body{margin:0;padding:0;background:#ffffff;font-size:14px}\n\
         .grid3{display:grid;grid-template-columns:1fr 1fr 1fr;gap:8px;width:600px;margin:8px}\n\
         .fill{display:grid;grid-template-columns:repeat(auto-fill,minmax(90px,1fr));gap:6px;width:600px;margin:8px}\n\
         .spans{display:grid;grid-template-columns:100px 100px 100px;grid-auto-rows:30px;gap:6px;width:600px;margin:8px}\n\
         .cell{background:#dde6f2;color:#101820;padding:4px;min-height:18px}\n\
         .wide{grid-column:span 2}\n\
         .tall{grid-row:span 2}\n",
    );
    html.push_str("</style></head><body>");
    html.push_str("<div class=\"grid3\">");
    for i in 0..9 {
        let _ = write!(html, "<div class=\"cell\">cell {i}</div>");
    }
    html.push_str("</div>");
    html.push_str("<div class=\"fill\">");
    for i in 0..12 {
        let _ = write!(html, "<div class=\"cell\">auto {i}</div>");
    }
    html.push_str("</div>");
    html.push_str("<div class=\"spans\">");
    for i in 0..8 {
        let class = if i == 0 {
            "cell wide"
        } else if i == 2 {
            "cell tall"
        } else {
            "cell"
        };
        let _ = write!(html, "<div class=\"{class}\">span {i}</div>");
    }
    html.push_str("</div></body></html>\n");
    html
}

/// Author stylesheet for the `css/vars-calc-10k` row: `:root` custom
/// properties, `var()` uses in colors/sizes/margins and simple `calc()`
/// expressions in lengths. Deterministic and self-contained.
pub fn vars_calc_css() -> String {
    let mut css = String::with_capacity(8192);
    css.push_str(
        ":root{--ink:#1d3557;--accent:#e63946;--pad:6px;--gap:4px;--width:140px}\n\
         body{font-size:16px;color:var(--ink)}\n\
         #app{padding:calc(4px + 2px)}\n\
         .item{color:var(--ink);padding:var(--pad);width:calc(140px + 10px);\
         margin:calc(3px + 1px);background:#f1faee}\n\
         .item span.label{color:var(--accent);font-size:calc(10px + 2px);\
         padding:var(--gap)}\n",
    );
    for i in 0..64usize {
        let _ = writeln!(
            css,
            ".c{i}{{padding:calc({}px + 2px);color:var(--ink)}}",
            i % 9
        );
    }
    for i in 0..16usize {
        let _ = writeln!(
            css,
            "span.s{i}{{margin:calc({}px + 1px);font-size:calc(9px + 3px)}}",
            i % 5
        );
    }
    css
}

/// Target size of the sticky/text-transform layout fixture (~1 MiB).
pub const STICKY_TEXT_TARGET_BYTES: usize = 1_048_576;

/// Deterministic page of at least `target_bytes` with `position: sticky`
/// headers and `text-transform` paragraphs.
///
/// Feeds the `layout/sticky-text-1mb` row: style resolution happens once
/// outside the timed loop and the benchmark measures `layout_document` only.
/// No network, no scripts, no randomness.
pub fn sticky_text_page(target_bytes: usize) -> String {
    let mut html = String::with_capacity(target_bytes + target_bytes / 2);
    html.push_str(
        "<!DOCTYPE html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">\
         <title>Sticky + text-transform</title>\n<style>\n\
         html,body{margin:0;padding:0;background:#ffffff;font-size:14px}\n\
         h2.sticky{position:sticky;top:0;margin:0;padding:6px;background:#dde6f2;\
         border-bottom:1px solid #b8c6d9}\n\
         p{margin:4px 0;line-height:1.4}\n\
         .upper{text-transform:uppercase}\n\
         .lower{text-transform:lowercase}\n\
         .cap{text-transform:capitalize}\n\
         .card{padding:6px;margin:4px;border:1px solid #cccccc;background:#f7f9fc}\n\
         .card.sticky-card{position:sticky;top:28px}\n\
         ul{margin:4px 0 4px 18px}\n",
    );
    html.push_str("</style></head><body>\n");
    let mut section = 0usize;
    while html.len() < target_bytes {
        let _ = writeln!(html, "<section>");
        let _ = writeln!(
            html,
            "<h2 class=\"sticky\">Section {section} sticky header</h2>"
        );
        let _ = writeln!(
            html,
            "<p class=\"upper\">uppercase text for section {section} with words and numbers {section}</p>"
        );
        let _ = writeln!(
            html,
            "<p class=\"lower\">LOWERCASE TEXT FOR SECTION {section} WITH CAPITALS AND WORDS</p>"
        );
        let _ = writeln!(
            html,
            "<p class=\"cap\">capitalize each word in section {section} for typography coverage</p>"
        );
        let _ = writeln!(
            html,
            "<div class=\"card sticky-card\"><b>sticky card {section}</b> with mixed <i>italic</i> and <u>underline</u> content</div>"
        );
        html.push_str("<ul>");
        for item in 0..4usize {
            let _ = write!(
                html,
                "<li class=\"upper\">item {item} of section {section}</li>"
            );
        }
        html.push_str("</ul>\n</section>\n");
        section += 1;
    }
    html.push_str("</body></html>\n");
    html
}

/// Deterministic HTML page with `boxes` rounded, clipped boxes and gradient
/// `background-clip:text` headings: the paint paths for `border-radius` +
/// `overflow:hidden` and text clipping, at 800×600.
pub fn clipping_page(boxes: usize) -> String {
    let mut html = String::with_capacity(boxes.saturating_mul(220).saturating_add(1024));
    html.push_str(
        "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\"><title>Clipping</title>\n<style>\n\
         html,body{margin:0;padding:0;background:#ffffff}\n\
         .wrap{display:flex;flex-wrap:wrap;width:800px}\n\
         .clip{width:150px;height:110px;margin:6px;border-radius:28px;overflow:hidden;\
         background:#ddeeff}\n\
         .clip .inner{width:240px;height:180px;background:#ff8800;color:#101820;\
         font-size:18px;padding:6px}\n\
         .grad{font-size:38px;margin:8px;color:transparent;\
         background-image:linear-gradient(90deg,#ff0000,#0000ff);\
         background-clip:text}\n",
    );
    html.push_str("</style></head><body><div class=\"wrap\">");
    for i in 0..boxes {
        let _ = write!(
            html,
            "<div class=\"clip\"><div class=\"inner\">clipped content {i} that overflows the rounded box</div></div>",
        );
    }
    html.push_str("</div>");
    for i in 0..6 {
        let _ = write!(html, "<div class=\"grad\">gradient text {i}</div>");
    }
    html.push_str("</body></html>\n");
    html
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
/// Number of gradient boxes used by the E2E gradients pipeline row.
pub const GRADIENT_BOXES: usize = 60;
/// Element count used by the E2E custom-properties/`calc()` row.
pub const VARS_CALC_NODES: usize = 10_000;
/// Number of rounded/clipped boxes used by the E2E clipping paint row.
pub const CLIPPING_BOXES: usize = 48;
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
        assert_eq!(gradients_page(8), gradients_page(8));
        assert_eq!(grid_page(), grid_page());
        assert_eq!(vars_calc_css(), vars_calc_css());
        assert_eq!(clipping_page(8), clipping_page(8));
        assert_eq!(sticky_text_page(4096), sticky_text_page(4096));
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

        let gradients = gradients_page(9);
        assert_eq!(gradients.matches("background-image").count(), 9);
        assert!(gradients.contains("linear-gradient("));
        assert!(gradients.contains("radial-gradient("));

        let grids = grid_page();
        assert_eq!(grids.matches("display:grid").count(), 3);
        assert!(grids.contains("repeat(auto-fill"));
        assert!(grids.contains("grid-column:span 2"));
        assert_eq!(grids.matches("class=\"cell").count(), GRID_CELLS);

        let vars = vars_calc_css();
        assert!(vars.contains(":root{"));
        assert!(vars.contains("var(--ink)"));
        assert!(vars.contains("calc("));

        let clip = clipping_page(6);
        assert_eq!(clip.matches("class=\"clip\"").count(), 6);
        assert!(clip.contains("border-radius:28px"));
        assert!(clip.contains("overflow:hidden"));
        assert!(clip.contains("background-clip:text"));

        let sticky = sticky_text_page(4096);
        assert!(sticky.len() >= 4096);
        assert!(sticky.contains("position:sticky"));
        assert!(sticky.contains("text-transform:uppercase"));
        assert!(sticky.contains("text-transform:lowercase"));
        assert!(sticky.contains("text-transform:capitalize"));
        assert!(sticky.ends_with("</html>\n"));
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
