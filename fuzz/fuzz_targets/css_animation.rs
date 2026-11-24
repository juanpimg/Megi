#![no_main]
//! Fuzz target: `@keyframes` / transition declarations and animation runtime.
//!
//! `css_parse` covers the generic stylesheet tokenizer; this target drives the values that
//! only appear in animation code paths: time, timing function, iteration count, direction,
//! fill mode, play state and transition property, plus the cascade/animation runtime that
//! consumes `@keyframes` and advances a timeline. The CSS specs (CSS Animations 1 §3,
//! CSS Transitions 1 §2, CSS Easing 1 §2) define the accepted grammars.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);

    // Parser entry points for the declaration values; arbitrary text is expected to fail.
    let stylesheet = megi_css::parse_stylesheet(&text);
    let _ = stylesheet.keyframes.len();
    let _ = megi_css::parse_list_of_declarations(&text).len();

    let mut tokens = text.split_whitespace().take(16);
    let _ = megi_css::animation::parse_time_ms(tokens.next().unwrap_or(""));
    let _ = megi_css::animation::parse_iteration_count(tokens.next().unwrap_or(""));
    let _ = megi_css::animation::parse_direction(tokens.next().unwrap_or(""));
    let _ = megi_css::animation::parse_fill_mode(tokens.next().unwrap_or(""));
    let _ = megi_css::animation::parse_play_state(tokens.next().unwrap_or(""));
    let _ = megi_css::animation::parse_transition_property(tokens.next().unwrap_or(""));
    if let Some(timing) = megi_css::animation::parse_timing_function(tokens.next().unwrap_or("")) {
        let x = (data.first().copied().unwrap_or(0) as f32) / 255.0;
        let _ = megi_css::animation_runtime::evaluate_timing_function(&timing, x);
        let _ = megi_css::animation_runtime::evaluate_timing_function_before(&timing, x, true);
    }

    // A tiny DOM plus the fuzzed stylesheet, so @keyframes are compiled into the collection
    // and the runtime actually walks animation/transition declarations at derived times.
    let mut doc = megi_core::Document::new();
    let html = doc.create_element(megi_core::Namespace::Html, "html", vec![]);
    let body = doc.create_element(megi_core::Namespace::Html, "body", vec![]);
    let div = doc.create_element(
        megi_core::Namespace::Html,
        "div",
        vec![megi_core::Attribute::new("class", "target")],
    );
    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, div);

    let mut collection = megi_css::StyleCollection::default();
    collection.add_css(&text, megi_css::Origin::Author);
    let mut styles =
        megi_css::resolve_document_styles_with_viewport(&doc, Some(&text), 800.0, 600.0);
    let time_ms = (data.first().copied().unwrap_or(0) as f32) * 37.0;
    megi_css::apply_animations_to_styles(&doc, &mut styles, &collection, time_ms);
    let from_styles = styles.clone();
    megi_css::apply_transitions_to_styles(&doc, &from_styles, &mut styles, time_ms);

    let mut pseudo_styles = megi_css::PseudoStyles::new();
    let from_pseudos = pseudo_styles.clone();
    megi_css::apply_animations_to_pseudo_styles(&mut pseudo_styles, &collection, time_ms);
    megi_css::apply_transitions_to_pseudo_styles(&from_pseudos, &mut pseudo_styles, time_ms);
});
