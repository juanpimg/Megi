//! Submodule of [`super`]: build extracted from the document model.

use super::*;

impl Document {
    /// Builds the model from the parsed XML root.
    pub fn build(root: &XmlElement, limits: &Limits, svg_time: f32) -> Result<Self, SvgError> {
        let animated = apply_smil(root, svg_time)?;
        let root = animated.as_ref().unwrap_or(root);
        let root_name = root.local_name_lower();
        if root_name != "svg" {
            return Err(SvgError::Invalid(format!(
                "root element is <{}>, expected <svg>",
                root.name
            )));
        }
        let view_box = parse_view_box(root.attribute("viewBox"));
        let width = parse_length(root.attribute("width"), 0.0)
            .or_else(|| view_box.map(|view| view.width))
            .unwrap_or(300.0);
        let height = parse_length(root.attribute("height"), 0.0)
            .or_else(|| view_box.map(|view| view.height))
            .unwrap_or(150.0);
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return Err(SvgError::Invalid("non-positive intrinsic size".to_owned()));
        }
        let preserve = parse_preserve_aspect_ratio(root.attribute("preserveAspectRatio"));
        let defs = collect_defs(root);
        let rules = parse_stylesheet(&collect_style_text(root));
        let (gradients, gradient_ids) = build_gradients(&defs);
        let mut pattern_ids = HashMap::new();
        let pattern_elements = collect_pattern_elements(&defs);
        for (index, (id, _)) in pattern_elements.iter().enumerate() {
            pattern_ids.insert(id.clone(), index as u32);
        }
        let clip_elements = collect_effect_elements(&defs, "clippath");
        let mask_elements = collect_effect_elements(&defs, "mask");
        let filter_elements = collect_effect_elements(&defs, "filter");
        let clip_ids = id_map(&clip_elements);
        let mask_ids = id_map(&mask_elements);
        let filter_ids = id_map(&filter_elements);
        // One document-wide selector budget: the clip/mask/pattern builders below each use
        // their own `Builder`, and without sharing the counter a page could stack thousands
        // of them under one stylesheet.
        let mut selector_checks = 0usize;
        let clip_paths = build_clip_paths(
            &defs,
            &rules,
            &gradient_ids,
            &pattern_ids,
            &clip_elements,
            limits,
            &mut selector_checks,
        )?;
        let masks = build_masks(
            &defs,
            &rules,
            &gradient_ids,
            &pattern_ids,
            &mask_elements,
            limits,
            &mut selector_checks,
        )?;
        let filters = build_filters(&filter_elements);
        let mut builder = Builder {
            defs: &defs,
            rules: &rules,
            gradient_ids: &gradient_ids,
            pattern_ids: &pattern_ids,
            clip_ids: &clip_ids,
            mask_ids: &mask_ids,
            filter_ids: &filter_ids,
            clip_paths: &clip_paths,
            masks: &masks,
            filters: &filters,
            limits,
            selector_checks_used: selector_checks,
            shapes: Vec::new(),
            texts: Vec::new(),
            use_stack: Vec::new(),
            use_expansions: 0,
            active_clips: Vec::new(),
            active_masks: Vec::new(),
            active_filter: None,
            group_bbox_stack: Vec::new(),
            segments_used: 0,
        };
        let mut style = Style::default();
        let mut selector_budget = SelectorBudget {
            used: 0,
            max: limits.max_selector_checks,
        };
        style.apply(
            root,
            &rules,
            &[],
            &gradient_ids,
            &pattern_ids,
            &mut selector_budget,
        )?;
        // #1659: the root must accept percentage opacity like every other element.
        let root_opacity = root
            .attribute("opacity")
            .and_then(parse_opacity)
            .unwrap_or(1.0);
        let root_transform = Transform::IDENTITY;
        let mut ancestors: Vec<&XmlElement> = vec![root];
        builder.visit_children(
            root,
            &mut ancestors,
            root_transform,
            &style,
            root_opacity,
            0,
        )?;
        selector_checks = builder.selector_checks_used;
        let patterns = build_patterns(
            &defs,
            &rules,
            &gradient_ids,
            &pattern_ids,
            limits,
            &mut selector_checks,
        )?;
        Ok(Self {
            width,
            height,
            view_box,
            preserve,
            shapes: builder.shapes,
            gradients,
            patterns,
            texts: builder.texts,
            clip_paths,
            masks,
            filters,
        })
    }
}

/// Collects every element that has an `id` for `<use>` resolution.
fn collect_defs(root: &XmlElement) -> HashMap<String, &XmlElement> {
    let mut defs = HashMap::new();
    let mut stack = vec![root];
    while let Some(element) = stack.pop() {
        if let Some(id) = element.attribute("id") {
            defs.entry(id.to_owned()).or_insert(element);
        }
        for child in element.children.iter().rev() {
            if let XmlNode::Element(child) = child {
                stack.push(child);
            }
        }
    }
    defs
}
