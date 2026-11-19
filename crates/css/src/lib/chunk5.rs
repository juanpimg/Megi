//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;

#[cfg(test)]
pub(crate) mod tests_part4 {
    #[allow(unused_imports)]
    use super::*;
    #[allow(unused_imports)]
    use crate::lib_tests_helpers::*;
    #[allow(unused_imports)]
    use megi_core::{Attribute, Document, Namespace};
    #[test]
    fn test_line_height_number_factor_and_inheritance() {
        // Parent: font-size 16px, line-height 1.5 (unitless factor, computed px 24).
        let (parent, child) = style_pair("font-size: 16px; line-height: 1.5;", "font-size: 32px;");
        assert_eq!(parent.line_height, Some(24.0));
        assert_eq!(parent.line_height_factor, Some(1.5));

        // The child inherits the *factor*; layout re-resolves 1.5 * 32px = 48px. CSS keeps the
        // inherited computed px (24) for backwards compatibility.
        assert_eq!(child.line_height_factor, Some(1.5));
        assert_eq!(child.line_height, Some(24.0));

        // Absolute lengths and `normal` are not factors.
        let px = style_with_inline("div", "line-height: 24px;");
        assert_eq!(px.line_height, Some(24.0));
        assert_eq!(px.line_height_factor, None);

        let normal = style_with_inline("div", "line-height: normal;");
        assert_eq!(normal.line_height, Some(16.0 * 1.2));
        assert_eq!(normal.line_height_factor, None);

        // Percentages now compute against the element font-size (and are not a factor).
        let pct = style_with_inline("div", "font-size: 20px; line-height: 150%;");
        assert_eq!(pct.line_height, Some(30.0));
        assert_eq!(pct.line_height_factor, None);

        // `inherit_from` copies the factor too.
        let mut inherited = ComputedStyle::initial();
        inherited.inherit_from(&parent);
        assert_eq!(inherited.line_height_factor, Some(1.5));
        assert_eq!(inherited.line_height, Some(24.0));
    }

    #[test]
    fn test_cascade_details_inline_important_root_and_attribute_operators() {
        let mut doc = Document::new();
        let html = doc.create_element(Namespace::Html, "html", vec![]);
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        doc.append_child(doc.root(), html);
        doc.append_child(html, body);

        // Inline `!important` beats an author `!important` declaration (inline specificity).
        let important = doc.create_element(
            Namespace::Html,
            "div",
            vec![Attribute::new("style", "color: red !important;")],
        );
        doc.append_child(body, important);
        let styles = resolve_document_styles(&doc, Some("div { color: blue !important; }"));
        assert_eq!(styles.get(&important).expect("style").color, Color::RED);

        // A normal inline declaration loses against an author `!important` declaration.
        let normal = doc.create_element(
            Namespace::Html,
            "div",
            vec![Attribute::new("style", "color: red;")],
        );
        doc.append_child(body, normal);
        let styles = resolve_document_styles(&doc, Some("div { color: blue !important; }"));
        assert_eq!(styles.get(&normal).expect("style").color, Color::BLUE);

        // `:root` matches the document element only (the non-inherited property shows it).
        let styles = resolve_document_styles(&doc, Some(":root { margin-top: 7px; }"));
        assert_eq!(
            styles.get(&html).expect("html style").margin.top,
            Length::Px(7.0)
        );
        assert_eq!(
            styles.get(&body).expect("body style").margin.top,
            Length::Px(8.0)
        );

        // Attribute selectors: `~=`, `|=`, `^=`, `$=` and `*=` beyond the `=` already covered.
        let element = doc.create_element(
            Namespace::Html,
            "p",
            vec![
                Attribute::new("data-words", "alpha beta"),
                Attribute::new("lang", "en-US"),
                Attribute::new("data-prefix", "megi-core"),
                Attribute::new("data-suffix", "engine.css"),
                Attribute::new("data-sub", "x-megi-y"),
            ],
        );
        doc.append_child(body, element);
        for selector in [
            "[data-words~=\"beta\"]",
            "[lang|=\"en\"]",
            "[data-prefix^=\"megi\"]",
            "[data-suffix$=\".css\"]",
            "[data-sub*=\"megi\"]",
        ] {
            assert!(
                parse_selectors(selector).matches(&doc, element),
                "{selector} should match"
            );
        }
        for selector in [
            "[data-words~=\"gamma\"]",
            "[lang|=\"fr\"]",
            "[data-prefix^=\"core\"]",
            "[data-suffix$=\".png\"]",
            "[data-sub*=\"nope\"]",
        ] {
            assert!(
                !parse_selectors(selector).matches(&doc, element),
                "{selector} should not match"
            );
        }
    }

    // ---------------------------------------------------------------------------------------

    // Round 5: additive table foundations (display table-*, border-spacing, border-collapse,

    // table-layout, caption-side, empty-cells and the UA table rules of WHATWG HTML §15).

    // ---------------------------------------------------------------------------------------

    #[test]
    fn test_display_table_variants_parse() {
        for (value, expected) in [
            ("table", Display::Table),
            ("inline-table", Display::InlineTable),
            ("table-row-group", Display::TableRowGroup),
            ("table-header-group", Display::TableHeaderGroup),
            ("table-footer-group", Display::TableFooterGroup),
            ("table-row", Display::TableRow),
            ("table-column-group", Display::TableColumnGroup),
            ("table-column", Display::TableColumn),
            ("table-cell", Display::TableCell),
            ("table-caption", Display::TableCaption),
        ] {
            assert_eq!(Display::parse(value), Some(expected), "display: {value}");
        }
        // Existing values keep working and unknown values stay invalid.
        assert_eq!(Display::parse("block"), Some(Display::Block));
        assert_eq!(Display::parse("list-item"), Some(Display::ListItem));
        assert_eq!(Display::parse("table-foo"), None);
    }

    #[test]
    fn test_table_properties_parse_and_initial() {
        let initial = ComputedStyle::initial();
        assert_eq!(initial.border_spacing, (0.0, 0.0));
        assert_eq!(initial.border_collapse, BorderCollapse::Separate);
        assert_eq!(initial.table_layout, TableLayout::Auto);
        assert_eq!(initial.caption_side, CaptionSide::Top);
        assert_eq!(initial.empty_cells, EmptyCells::Show);

        // One or two lengths; font-relative lengths resolve against the font-size.
        assert_eq!(
            style_with_inline("table", "border-spacing: 3px;").border_spacing,
            (3.0, 3.0)
        );
        assert_eq!(
            style_with_inline("table", "border-spacing: 1px 4px;").border_spacing,
            (1.0, 4.0)
        );
        assert_eq!(
            style_with_inline("table", "border-spacing: 0.5em 1em;").border_spacing,
            (8.0, 16.0)
        );
        // Explicit `0` is a valid specified value (CSS 2.1 §17.6.1), not "absent"; it must
        // override the UA table default of 2px (regression #1703).
        assert_eq!(
            style_with_inline("table", "border-spacing: 0;").border_spacing,
            (0.0, 0.0)
        );
        assert_eq!(
            style_with_inline("table", "border-spacing: 0px 0px;").border_spacing,
            (0.0, 0.0)
        );

        // `div` has no UA `border-spacing`, so invalid values fall back to the initial value.
        for invalid in [
            "border-spacing: -1px;",
            "border-spacing: 10%;",
            "border-spacing: auto;",
        ] {
            assert_eq!(
                style_with_inline("div", invalid).border_spacing,
                (0.0, 0.0),
                "{invalid}"
            );
        }

        let table = style_with_inline(
            "table",
            "border-collapse: collapse; table-layout: fixed; \
             caption-side: bottom; empty-cells: hide;",
        );
        assert_eq!(table.border_collapse, BorderCollapse::Collapse);
        assert_eq!(table.table_layout, TableLayout::Fixed);
        assert_eq!(table.caption_side, CaptionSide::Bottom);
        assert_eq!(table.empty_cells, EmptyCells::Hide);

        // Enum parsers.
        assert_eq!(
            BorderCollapse::parse("separate"),
            Some(BorderCollapse::Separate)
        );
        assert_eq!(
            BorderCollapse::parse("collapse"),
            Some(BorderCollapse::Collapse)
        );
        assert_eq!(BorderCollapse::parse("nope"), None);
        assert_eq!(TableLayout::parse("fixed"), Some(TableLayout::Fixed));
        assert_eq!(CaptionSide::parse("bottom"), Some(CaptionSide::Bottom));
        assert_eq!(EmptyCells::parse("hide"), Some(EmptyCells::Hide));

        // None of the table properties are inherited.
        let (parent, child) = style_pair(
            "border-spacing: 5px; border-collapse: collapse; table-layout: fixed; \
             caption-side: bottom; empty-cells: hide;",
            "",
        );
        assert_eq!(parent.border_spacing, (5.0, 5.0));
        assert_eq!(child.border_spacing, (0.0, 0.0));
        assert_eq!(child.border_collapse, BorderCollapse::Separate);
        assert_eq!(child.table_layout, TableLayout::Auto);
        assert_eq!(child.caption_side, CaptionSide::Top);
        assert_eq!(child.empty_cells, EmptyCells::Show);
    }

    #[test]
    fn test_ua_table_stylesheet() {
        let mut doc = Document::new();
        let html = doc.create_element(Namespace::Html, "html", vec![]);
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        let table = doc.create_element(Namespace::Html, "table", vec![]);
        let caption = doc.create_element(Namespace::Html, "caption", vec![]);
        let colgroup = doc.create_element(Namespace::Html, "colgroup", vec![]);
        let col = doc.create_element(Namespace::Html, "col", vec![]);
        let thead = doc.create_element(Namespace::Html, "thead", vec![]);
        let head_row = doc.create_element(Namespace::Html, "tr", vec![]);
        let th = doc.create_element(Namespace::Html, "th", vec![]);
        let tbody = doc.create_element(Namespace::Html, "tbody", vec![]);
        let body_row = doc.create_element(Namespace::Html, "tr", vec![]);
        let td = doc.create_element(Namespace::Html, "td", vec![]);
        let tfoot = doc.create_element(Namespace::Html, "tfoot", vec![]);
        let foot_row = doc.create_element(Namespace::Html, "tr", vec![]);
        let foot_cell = doc.create_element(Namespace::Html, "td", vec![]);

        doc.append_child(doc.root(), html);
        doc.append_child(html, body);
        doc.append_child(body, table);
        doc.append_child(table, caption);
        doc.append_child(table, colgroup);
        doc.append_child(colgroup, col);
        doc.append_child(table, thead);
        doc.append_child(thead, head_row);
        doc.append_child(head_row, th);
        doc.append_child(table, tbody);
        doc.append_child(tbody, body_row);
        doc.append_child(body_row, td);
        doc.append_child(table, tfoot);
        doc.append_child(tfoot, foot_row);
        doc.append_child(foot_row, foot_cell);

        let styles = resolve_document_styles(&doc, None);
        let style = |node| styles.get(&node).expect("table style");

        assert_eq!(style(table).display, Display::Table);
        assert_eq!(style(table).border_spacing, (2.0, 2.0));
        assert_eq!(style(table).border_collapse, BorderCollapse::Separate);
        assert_eq!(style(table).box_sizing, BoxSizing::BorderBox);
        // WHATWG §15: table border-color is gray.
        assert_eq!(
            style(table).border_color,
            RectEdges::all(Color::new(128, 128, 128, 255))
        );

        assert_eq!(style(caption).display, Display::TableCaption);
        assert_eq!(style(caption).text_align, TextAlign::Center);
        assert_eq!(style(colgroup).display, Display::TableColumnGroup);
        assert_eq!(style(col).display, Display::TableColumn);
        assert_eq!(style(thead).display, Display::TableHeaderGroup);
        assert_eq!(style(tbody).display, Display::TableRowGroup);
        assert_eq!(style(tfoot).display, Display::TableFooterGroup);
        assert_eq!(style(head_row).display, Display::TableRow);
        assert_eq!(style(body_row).display, Display::TableRow);
        assert_eq!(style(th).display, Display::TableCell);
        assert_eq!(style(td).display, Display::TableCell);
        assert_eq!(style(th).padding.top, Length::Px(1.0));
        assert_eq!(style(th).font_weight, 700);
        assert_eq!(style(th).text_align, TextAlign::Center);
        assert_eq!(style(td).padding.left, Length::Px(1.0));
    }

    // ---------------------------------------------------------------------------------------

    // Round 6: `@import` parsing and full media query lists.

    // ---------------------------------------------------------------------------------------

    #[test]
    fn test_parse_stylesheet_imports() {
        let sheet = parse_stylesheet(
            "@import url(\"base.css\");\n@import \"print.css\" print;\nbody { color: red; }",
        );
        assert_eq!(sheet.imports.len(), 2);
        assert_eq!(sheet.imports[0].url, "base.css");
        assert!(sheet.imports[0].media.is_empty());
        assert_eq!(sheet.imports[1].url, "print.css");
        assert_eq!(sheet.rules.len(), 1);

        // The import media list is parsed and can be evaluated by consumers.
        let screen_context = MediaContext::default();
        let print_context = MediaContext {
            viewport: None,
            screen: false,
            color_scheme: ColorScheme::Light,
            reduced_motion: false,
        };
        assert!(!media::evaluate(&sheet.imports[1].media, &screen_context));
        assert!(media::evaluate(&sheet.imports[1].media, &print_context));

        // Unquoted url() form plus a media condition.
        let sheet = parse_stylesheet("@import url(theme.css) screen and (min-width: 600px);");
        assert_eq!(sheet.imports.len(), 1);
        assert_eq!(sheet.imports[0].url, "theme.css");
        assert!(media::evaluate(
            &sheet.imports[0].media,
            &MediaContext::with_viewport(800.0, 600.0)
        ));
        assert!(!media::evaluate(
            &sheet.imports[0].media,
            &MediaContext::with_viewport(400.0, 600.0)
        ));

        // Imports after other rules are ignored, and unsupported forms are not exposed.
        let sheet = parse_stylesheet("body { color: red; } @import \"late.css\";");
        assert!(sheet.imports.is_empty());
        assert_eq!(sheet.rules.len(), 1);
        let sheet = parse_stylesheet("@import layer(base.css); p { color: red; }");
        assert!(sheet.imports.is_empty());
    }

    #[test]
    fn test_import_stylesheet_media_gating() {
        let imported = parse_stylesheet("p { color: red; }");
        let mut collection = StyleCollection::new();
        collection.media = MediaContext::with_viewport(800.0, 600.0);

        let screen_import = ImportRule {
            url: "a.css".to_string(),
            media: parse_media_query_list("screen and (min-width: 700px)"),
        };
        assert!(collection.add_import_stylesheet(&screen_import, &imported, Origin::Author));

        let print_import = ImportRule {
            url: "b.css".to_string(),
            media: parse_media_query_list("print"),
        };
        assert!(!collection.add_import_stylesheet(&print_import, &imported, Origin::Author));
        assert_eq!(collection.rules.len(), 1);

        // The added imported sheet participates in the cascade.
        let mut doc = Document::new();
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        let p = doc.create_element(Namespace::Html, "p", vec![]);
        doc.append_child(doc.root(), body);
        doc.append_child(body, p);
        assert!(
            collection.rules[0].selectors.matches(&doc, p),
            "imported rule should be indexed and match"
        );
    }
}
