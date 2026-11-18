//! Submodule of [`super`]: visit extracted from the document model.

use super::*;

impl<'a> Builder<'a> {
    pub(crate) fn visit_children(
        &mut self,
        element: &'a XmlElement,
        ancestors: &mut Vec<&'a XmlElement>,
        transform: Transform,
        style: &Style,
        opacity: f32,
        depth: usize,
    ) -> Result<(), SvgError> {
        for child in &element.children {
            if let XmlNode::Element(child) = child {
                self.visit(child, ancestors, transform, style, opacity, depth + 1)?;
            }
        }
        Ok(())
    }

    fn visit(
        &mut self,
        element: &'a XmlElement,
        ancestors: &mut Vec<&'a XmlElement>,
        transform: Transform,
        parent_style: &Style,
        group_opacity: f32,
        depth: usize,
    ) -> Result<(), SvgError> {
        if depth > self.limits.max_depth {
            return Err(SvgError::Limit("SVG element nesting too deep".to_owned()));
        }
        if self.shapes.len() >= self.limits.max_shapes {
            return Err(SvgError::Limit("too many SVG shapes".to_owned()));
        }
        let name = element.local_name_lower();
        let mut style = parent_style.clone();
        // `clip-path`/`mask`/`filter` are not inherited: descendants only see them through
        // the active-effect stack, so clear any value copied from the parent style.
        style.clip_path = None;
        style.mask = None;
        style.filter = None;
        let mut selector_budget = SelectorBudget {
            used: self.selector_checks_used,
            max: self.limits.max_selector_checks,
        };
        style.apply(
            element,
            self.rules,
            ancestors,
            self.gradient_ids,
            self.pattern_ids,
            &mut selector_budget,
        )?;
        // All declarations are applied now, including `color` from CSS/inline styles.
        style.resolve_current_color();
        self.selector_checks_used = selector_budget.used;
        let local_transform = element
            .attribute("transform")
            .and_then(Transform::parse)
            .filter(|transform| transform.is_finite())
            .unwrap_or(Transform::IDENTITY);
        let transform = transform.then(local_transform);
        let opacity = group_opacity * parse_opacity_attribute(element, "opacity").unwrap_or(1.0);
        ancestors.push(element);
        // `display: none` removes the whole subtree (#37).
        if !style.display {
            ancestors.pop();
            return Ok(());
        }
        let visible = style.visibility;
        let result = if visible {
            match name.as_str() {
                "svg" => {
                    let viewport = nested_viewport_transform(element);
                    let transform = transform.then(viewport);
                    let (clips, masks, filter) = self.own_effects(&style, transform, None, true);
                    let start = self.shapes.len();
                    let active = !clips.is_empty() || !masks.is_empty() || filter.is_some();
                    self.group_bbox_stack
                        .push(GroupBboxFrame { bbox: None, active });
                    let previous = self.push_effects(&clips, &masks, filter.as_ref());
                    let result =
                        self.visit_children(element, ancestors, transform, &style, opacity, depth);
                    self.pop_effects(previous);
                    let frame = self.group_bbox_stack.pop().expect("group bbox frame");
                    if active {
                        self.apply_group_bbox(start, &clips, &masks, filter.as_ref(), frame.bbox);
                    }
                    result
                }
                "g" | "a" | "switch" => {
                    let (clips, masks, filter) = self.own_effects(&style, transform, None, true);
                    let start = self.shapes.len();
                    let active = !clips.is_empty() || !masks.is_empty() || filter.is_some();
                    self.group_bbox_stack
                        .push(GroupBboxFrame { bbox: None, active });
                    let previous = self.push_effects(&clips, &masks, filter.as_ref());
                    let result =
                        self.visit_children(element, ancestors, transform, &style, opacity, depth);
                    self.pop_effects(previous);
                    let frame = self.group_bbox_stack.pop().expect("group bbox frame");
                    if active {
                        self.apply_group_bbox(start, &clips, &masks, filter.as_ref(), frame.bbox);
                    }
                    result
                }
                "defs" | "title" | "desc" | "metadata" | "style" | "linearGradient"
                | "radialGradient" | "pattern" | "clipPath" | "mask" | "filter" | "symbol"
                | "marker" | "tspan" | "image" | "foreignObject" | "script" | "stop" => Ok(()),
                "text" => self.push_text(element, transform, &style, opacity),
                "use" => {
                    let reference = element
                        .attribute("href")
                        .or_else(|| element.attribute("xlink:href"))
                        .unwrap_or_default();
                    let Some(id) = reference.strip_prefix('#') else {
                        ancestors.pop();
                        return Ok(());
                    };
                    let Some(target) = self.defs.get(id).copied() else {
                        ancestors.pop();
                        return Ok(());
                    };
                    if self.use_stack.iter().any(|seen| seen == id) {
                        ancestors.pop();
                        return Ok(());
                    }
                    if self.use_stack.len() >= self.limits.max_use_depth {
                        ancestors.pop();
                        return Err(SvgError::Limit(
                            "nested <use> expansion too deep".to_owned(),
                        ));
                    }
                    if self.use_expansions >= self.limits.max_use_expansions {
                        ancestors.pop();
                        return Err(SvgError::Limit("too many <use> expansions".to_owned()));
                    }
                    self.use_expansions += 1;
                    let x = parse_length(element.attribute("x"), 0.0).unwrap_or(0.0);
                    let y = parse_length(element.attribute("y"), 0.0).unwrap_or(0.0);
                    let use_transform = transform.then(Transform::translate(x, y));
                    let (clips, masks, filter) =
                        self.own_effects(&style, use_transform, None, true);
                    let start = self.shapes.len();
                    let active = !clips.is_empty() || !masks.is_empty() || filter.is_some();
                    self.group_bbox_stack
                        .push(GroupBboxFrame { bbox: None, active });
                    let previous = self.push_effects(&clips, &masks, filter.as_ref());
                    self.use_stack.push(id.to_owned());
                    let result = if target.local_name_lower() == "symbol" {
                        // A <symbol> is never painted directly; <use> instantiates its children
                        // through the symbol's viewBox/preserveAspectRatio viewport mapping (#38).
                        let symbol_transform =
                            use_transform.then(nested_viewport_transform(target));
                        self.visit_children(
                            target,
                            ancestors,
                            symbol_transform,
                            &style,
                            opacity,
                            depth + 1,
                        )
                    } else {
                        self.visit(target, ancestors, use_transform, &style, opacity, depth + 1)
                    };
                    self.use_stack.pop();
                    self.pop_effects(previous);
                    let frame = self.group_bbox_stack.pop().expect("group bbox frame");
                    if active {
                        self.apply_group_bbox(start, &clips, &masks, filter.as_ref(), frame.bbox);
                    }
                    result
                }
                _ => self.push_shape(element, &name, transform, &style, opacity),
            }
        } else {
            // Hidden subtrees are not painted, but descendants may opt back in with
            // `visibility: visible`, so keep walking the tree (#37).
            self.visit_children(element, ancestors, transform, &style, opacity, depth)
        };
        ancestors.pop();
        result
    }
}
