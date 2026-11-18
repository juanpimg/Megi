//! Submodule of [`super`]: paint extracted from the document model.

use super::*;

/// Collects `<pattern>` definitions in deterministic id order.
pub(crate) fn collect_pattern_elements<'a>(
    defs: &'a HashMap<String, &'a XmlElement>,
) -> Vec<(String, &'a XmlElement)> {
    let mut ids: Vec<&String> = defs.keys().collect();
    ids.sort();
    ids.into_iter()
        .filter_map(|id| {
            let element = defs[id];
            element
                .local_name_lower()
                .eq("pattern")
                .then(|| (id.clone(), element))
        })
        .collect()
}

/// Builds every `<pattern>`'s child shapes in deterministic id order.
pub(crate) fn build_patterns(
    defs: &HashMap<String, &XmlElement>,
    rules: &[StyleRule],
    gradient_ids: &HashMap<String, u32>,
    pattern_ids: &HashMap<String, u32>,
    limits: &Limits,
    checks: &mut usize,
) -> Result<Vec<PatternSpec>, SvgError> {
    let mut specs = Vec::new();
    for (_id, element) in collect_pattern_elements(defs) {
        let no_ids = HashMap::new();
        let no_clips: [ClipPathSpec; 0] = [];
        let no_masks: [MaskSpec; 0] = [];
        let no_filters: [FilterSpec; 0] = [];
        let mut builder = Builder {
            defs,
            rules,
            gradient_ids,
            pattern_ids,
            clip_ids: &no_ids,
            mask_ids: &no_ids,
            filter_ids: &no_ids,
            clip_paths: &no_clips,
            masks: &no_masks,
            filters: &no_filters,
            limits,
            selector_checks_used: *checks,
            shapes: Vec::new(),
            texts: Vec::new(),
            use_stack: Vec::new(),
            use_expansions: 0,
            active_clips: Vec::new(),
            active_masks: Vec::new(),
            segments_used: 0,
            active_filter: None,
            group_bbox_stack: Vec::new(),
        };
        let mut style = Style::default();
        let mut ancestors: Vec<&XmlElement> = vec![element];
        let mut selector_budget = SelectorBudget {
            used: builder.selector_checks_used,
            max: limits.max_selector_checks,
        };
        style.apply(
            element,
            rules,
            &ancestors,
            gradient_ids,
            pattern_ids,
            &mut selector_budget,
        )?;
        builder.selector_checks_used = selector_budget.used;
        builder.visit_children(element, &mut ancestors, Transform::IDENTITY, &style, 1.0, 0)?;
        *checks = builder.selector_checks_used;
        specs.push(PatternSpec {
            x: first_number(element.attribute("x")),
            y: first_number(element.attribute("y")),
            width: parse_length(element.attribute("width"), 0.0)
                .filter(|width| width.is_finite() && *width > 0.0)
                .unwrap_or(0.0),
            height: parse_length(element.attribute("height"), 0.0)
                .filter(|height| height.is_finite() && *height > 0.0)
                .unwrap_or(0.0),
            pattern_units: match element.attribute("patternUnits") {
                Some(raw) => Units::parse(Some(raw)),
                None => Units::ObjectBoundingBox,
            },
            content_units: Units::parse(element.attribute("patternContentUnits")),
            transform: element
                .attribute("patternTransform")
                .and_then(Transform::parse)
                .filter(|transform| transform.is_finite())
                .unwrap_or(Transform::IDENTITY),
            shapes: builder.shapes,
        });
    }
    Ok(specs)
}

/// Resolves every gradient definition once, in deterministic id order.
pub(crate) fn build_gradients(
    defs: &HashMap<String, &XmlElement>,
) -> (Vec<GradientSpec>, HashMap<String, u32>) {
    let mut ids: Vec<&String> = defs.keys().collect();
    ids.sort();
    let mut specs = Vec::new();
    let mut gradient_ids = HashMap::new();
    for id in ids {
        let element = defs[id];
        if !GradientSpec::is_gradient_element(element) {
            continue;
        }
        if let Some(spec) = GradientSpec::resolve(defs, id) {
            let index = specs.len() as u32;
            specs.push(spec);
            gradient_ids.insert(id.clone(), index);
        }
    }
    (specs, gradient_ids)
}
