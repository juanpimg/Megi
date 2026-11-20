use super::*;
use crate::xml::parse_document;

fn gradient(source: &str, id: &str) -> Option<GradientSpec> {
    let root = parse_document(source.as_bytes(), &crate::Limits::default()).expect("xml");
    let mut defs = HashMap::new();
    let mut stack = vec![&root];
    while let Some(element) = stack.pop() {
        if let Some(element_id) = element.attribute("id") {
            defs.insert(element_id.to_owned(), element);
        }
        for child in element.children.iter().rev() {
            if let XmlNode::Element(child) = child {
                stack.push(child);
            }
        }
    }
    if !defs.contains_key(id) {
        return None;
    }
    GradientSpec::resolve(&defs, id)
}

#[test]
fn decreasing_stop_offset_is_clamped_and_declaration_order_is_preserved() {
    // SVG 1.1 §13.2.4: a stop whose offset is below the previous one must be
    // clamped up to the previous offset, keeping declaration order. Sorting
    // (the old behaviour) inverted the colors (#1863).
    let source = "<svg><linearGradient id='g'>\
                  <stop offset='0' stop-color='red'/>\
                  <stop offset='0.5' stop-color='blue'/>\
                  <stop offset='0.25' stop-color='green'/>\
                  </linearGradient></svg>";
    let spec = gradient(source, "g").expect("gradient");
    assert_eq!(spec.stops.len(), 3);
    assert_eq!(spec.stops[0].offset, 0.0);
    assert_eq!(spec.stops[1].offset, 0.5);
    assert!(
        (spec.stops[2].offset - 0.5).abs() < 1e-6,
        "third stop must be clamped to the previous offset 0.5, got {}",
        spec.stops[2].offset
    );
    assert_eq!(spec.stops[1].color, Color::rgba(0, 0, 255, 255));
    assert_eq!(spec.stops[2].color, Color::rgba(0, 128, 0, 255));
    // Past the clamped region the last-declared stop (green) wins, not the
    // blue one that sorting would have placed last.
    assert_eq!(spec.evaluate(0.7), Color::rgba(0, 128, 0, 255));
}

#[test]
fn test_linear_defaults_and_stops_clamped() {
    let source = "<svg><linearGradient id='g'><stop offset='100%' stop-color='#0000ff'/>\
                  <stop offset='0' stop-color='#ff0000'/></linearGradient></svg>";
    let spec = gradient(source, "g").expect("gradient");
    assert_eq!(
        spec.geometry,
        GradientGeometry::Linear {
            x1: 0.0,
            y1: 0.0,
            x2: 1.0,
            y2: 0.0
        }
    );
    // SVG 1.1 §13.2.4: declaration order is preserved and the second stop's
    // offset is clamped up to the first (both 1.0), not reordered by offset.
    assert_eq!(spec.stops[0].offset, 1.0);
    assert_eq!(spec.stops[0].color, Color::rgba(0, 0, 255, 255));
    assert_eq!(spec.stops[1].offset, 1.0);
    assert_eq!(spec.stops[1].color, Color::rgba(255, 0, 0, 255));
    assert_eq!(spec.evaluate(0.0), Color::rgba(0, 0, 255, 255));
    // Both stops sit at 1.0; the first declared one wins the duplicate offset,
    // matching the raster crate's first-window semantics (#1416).
    assert_eq!(spec.evaluate(1.0), Color::rgba(0, 0, 255, 255));
}

#[test]
fn test_interpolation_and_stop_opacity() {
    let source = "<svg><linearGradient id='g'>\
                  <stop offset='0' stop-color='red' stop-opacity='0.5'/>\
                  <stop offset='1' stop-color='blue'/></linearGradient></svg>";
    let spec = gradient(source, "g").expect("gradient");
    let middle = spec.evaluate(0.5);
    assert!((middle.r as i32 - 128).abs() <= 2);
    assert!((middle.b as i32 - 128).abs() <= 2);
    assert!((middle.a as i32 - 191).abs() <= 2);
}

#[test]
fn test_spread_methods_clamp_reflect_repeat() {
    let source = "<svg><linearGradient id='g' spreadMethod='reflect'>\
                  <stop offset='0' stop-color='black'/>\
                  <stop offset='1' stop-color='white'/></linearGradient></svg>";
    let spec = gradient(source, "g").expect("gradient");
    assert_eq!(spec.apply_spread(1.25), 0.75);
    assert_eq!(spec.apply_spread(-0.25), 0.25);
    let source = source.replace("reflect", "repeat");
    let repeat = gradient(&source, "g").expect("gradient");
    assert!((repeat.apply_spread(1.25) - 0.25).abs() < 1e-6);
}

#[test]
fn test_href_inheritance_and_cycles() {
    let source = "<svg>\
        <linearGradient id='base'><stop offset='0' stop-color='red'/>\
        <stop offset='1' stop-color='blue'/></linearGradient>\
        <linearGradient id='child' href='#base' y2='100%'/></svg>";
    let child = gradient(source, "child").expect("inherited gradient");
    assert_eq!(child.stops.len(), 2);
    assert_eq!(
        child.geometry,
        GradientGeometry::Linear {
            x1: 0.0,
            y1: 0.0,
            x2: 1.0,
            y2: 1.0
        }
    );
    let cycle = "<svg><linearGradient id='a' href='#b'/><linearGradient id='b' href='#a'/></svg>";
    assert!(gradient(cycle, "a").is_none());
}

#[test]
fn test_radial_parameter_and_focus() {
    let source = "<svg><radialGradient id='g'><stop offset='0' stop-color='red'/>\
                  <stop offset='1' stop-color='blue'/></radialGradient></svg>";
    let spec = gradient(source, "g").expect("gradient");
    assert!((spec.parameter(0.5, 0.5) - 0.0).abs() < 1e-4);
    assert!((spec.parameter(1.0, 0.5) - 1.0).abs() < 1e-3);
    let far = spec.evaluate(1.5);
    assert_eq!(far, Color::rgba(0, 0, 255, 255));
}
