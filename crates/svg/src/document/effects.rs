//! Submodule of [`super`]: effects extracted from the document model.

use super::*;

/// Collects effect definitions (`clipPath`, `mask`, `filter`) in deterministic id order.
pub(crate) fn collect_effect_elements<'a>(
    defs: &'a HashMap<String, &'a XmlElement>,
    tag: &str,
) -> Vec<(String, &'a XmlElement)> {
    let mut ids: Vec<&String> = defs.keys().collect();
    ids.sort();
    ids.into_iter()
        .filter_map(|id| {
            let element = defs[id];
            (element.local_name_lower() == tag).then(|| (id.clone(), element))
        })
        .collect()
}

/// Assigns stable indices to effect definitions, in sorted-id order.
pub(crate) fn id_map(elements: &[(String, &XmlElement)]) -> HashMap<String, u32> {
    elements
        .iter()
        .enumerate()
        .map(|(index, (id, _))| (id.clone(), index as u32))
        .collect()
}

/// Flattens an effect element's children into shapes (clip/mask content).
fn build_effect_shapes(
    defs: &HashMap<String, &XmlElement>,
    rules: &[StyleRule],
    gradient_ids: &HashMap<String, u32>,
    pattern_ids: &HashMap<String, u32>,
    element: &XmlElement,
    limits: &Limits,
    checks: &mut usize,
) -> Result<Vec<Shape>, SvgError> {
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
        active_filter: None,
        group_bbox_stack: Vec::new(),
        segments_used: 0,
    };
    let style = Style {
        clip_path: None,
        mask: None,
        filter: None,
        ..Style::default()
    };
    let mut ancestors: Vec<&XmlElement> = vec![element];
    builder.visit_children(element, &mut ancestors, Transform::IDENTITY, &style, 1.0, 0)?;
    *checks = builder.selector_checks_used;
    Ok(builder.shapes)
}

/// Builds every `<clipPath>` (child geometry flattened in clip-path user space).
pub(crate) fn build_clip_paths(
    defs: &HashMap<String, &XmlElement>,
    rules: &[StyleRule],
    gradient_ids: &HashMap<String, u32>,
    pattern_ids: &HashMap<String, u32>,
    elements: &[(String, &XmlElement)],
    limits: &Limits,
    checks: &mut usize,
) -> Result<Vec<ClipPathSpec>, SvgError> {
    let mut specs = Vec::new();
    for (_id, element) in elements {
        let shapes = build_effect_shapes(
            defs,
            rules,
            gradient_ids,
            pattern_ids,
            element,
            limits,
            checks,
        )?;
        if shapes.len() > MAX_EFFECT_SHAPES {
            return Err(SvgError::Limit("too many shapes in clipPath".to_owned()));
        }
        let fill_rule = if element
            .attribute("clip-rule")
            .is_some_and(|value| value.eq_ignore_ascii_case("evenodd"))
        {
            FillRule::EvenOdd
        } else {
            FillRule::NonZero
        };
        specs.push(ClipPathSpec {
            units: Units::parse(element.attribute("clipPathUnits")),
            fill_rule,
            shapes,
        });
    }
    Ok(specs)
}

/// Parses `maskUnits`/`filterUnits` (their initial value is `objectBoundingBox`, unlike
/// `clipPathUnits`/`maskContentUnits`).
fn parse_object_box_units(raw: Option<&str>) -> Units {
    match raw.map(str::trim) {
        Some(value) if value.eq_ignore_ascii_case("userSpaceOnUse") => Units::UserSpaceOnUse,
        _ => Units::ObjectBoundingBox,
    }
}

/// Builds every `<mask>`.
pub(crate) fn build_masks(
    defs: &HashMap<String, &XmlElement>,
    rules: &[StyleRule],
    gradient_ids: &HashMap<String, u32>,
    pattern_ids: &HashMap<String, u32>,
    elements: &[(String, &XmlElement)],
    limits: &Limits,
    checks: &mut usize,
) -> Result<Vec<MaskSpec>, SvgError> {
    let mut specs = Vec::new();
    for (_id, element) in elements {
        let shapes = build_effect_shapes(
            defs,
            rules,
            gradient_ids,
            pattern_ids,
            element,
            limits,
            checks,
        )?;
        if shapes.len() > MAX_EFFECT_SHAPES {
            return Err(SvgError::Limit("too many shapes in mask".to_owned()));
        }
        specs.push(MaskSpec {
            mask_units: parse_object_box_units(element.attribute("maskUnits")),
            content_units: Units::parse(element.attribute("maskContentUnits")),
            x: RegionLength::parse(element.attribute("x"), RegionLength::Percent(-0.1)),
            y: RegionLength::parse(element.attribute("y"), RegionLength::Percent(-0.1)),
            width: RegionLength::parse(element.attribute("width"), RegionLength::Percent(1.2)),
            height: RegionLength::parse(element.attribute("height"), RegionLength::Percent(1.2)),
            shapes,
        });
    }
    Ok(specs)
}

/// Builds every `<filter>` (supported primitive subset: blur/offset/flood/composite).
fn filter_input(value: Option<&str>, default: FilterInput) -> FilterInput {
    match value.map(str::trim) {
        Some("SourceGraphic") | Some("sourceGraphic") => FilterInput::SourceGraphic,
        Some("SourceAlpha") | Some("sourceAlpha") => FilterInput::SourceAlpha,
        Some("*") | None | Some("") => default,
        Some(name) => FilterInput::Named(name.to_string()),
    }
}

fn filter_result(child: &XmlElement) -> Option<String> {
    child
        .attribute("result")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn parse_color_matrix(child: &XmlElement) -> Option<ColorMatrixKind> {
    match child
        .attribute("type")
        .map(|value| value.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("saturate") => {
            let value = first_number(child.attribute("values"));
            value
                .is_finite()
                .then_some(ColorMatrixKind::Saturate(value))
        }
        Some("huerotate") | Some("hueRotate") => {
            let value = first_number(child.attribute("values"));
            value
                .is_finite()
                .then_some(ColorMatrixKind::HueRotate(value))
        }
        Some("luminancetoalpha") | Some("luminanceToAlpha") => {
            Some(ColorMatrixKind::LuminanceToAlpha)
        }
        None | Some("matrix") => {
            let raw = child.attribute("values")?;
            let values: Vec<f32> = raw
                .split([' ', ',', '\t', '\n'])
                .filter_map(|part| part.trim().parse::<f32>().ok())
                .filter(|value| value.is_finite())
                .collect();
            if values.len() != 20 {
                return None;
            }
            let mut matrix = [0.0f32; 20];
            matrix.copy_from_slice(&values);
            Some(ColorMatrixKind::Matrix(matrix))
        }
        _ => None,
    }
}

fn parse_blend_mode(raw: Option<&str>) -> BlendMode {
    match raw
        .map(|value| value.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("multiply") => BlendMode::Multiply,
        Some("screen") => BlendMode::Screen,
        Some("darken") => BlendMode::Darken,
        Some("lighten") => BlendMode::Lighten,
        _ => BlendMode::Normal,
    }
}

fn parse_composite_operator(child: &XmlElement) -> CompositeOperator {
    match child
        .attribute("operator")
        .map(|value| value.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("in") => CompositeOperator::In,
        Some("out") => CompositeOperator::Out,
        Some("atop") => CompositeOperator::Atop,
        Some("arithmetic") => CompositeOperator::Arithmetic([
            first_number(child.attribute("k1")),
            first_number(child.attribute("k2")),
            first_number(child.attribute("k3")),
            first_number(child.attribute("k4")),
        ]),
        _ => CompositeOperator::Over,
    }
}

pub(crate) fn build_filters(elements: &[(String, &XmlElement)]) -> Vec<FilterSpec> {
    let mut specs = Vec::new();
    for (_id, element) in elements {
        let mut primitives = Vec::new();
        for child in &element.children {
            let XmlNode::Element(child) = child else {
                continue;
            };
            if primitives.len() >= MAX_FILTER_PRIMITIVES {
                break;
            }
            match child.local_name_lower().as_str() {
                "fegaussianblur" => {
                    let std_deviation = first_number(child.attribute("stdDeviation"));
                    if std_deviation.is_finite() && std_deviation > 0.0 {
                        primitives.push(FilterPrimitive::GaussianBlur {
                            std_deviation,
                            input: filter_input(child.attribute("in"), FilterInput::Previous),
                            result: filter_result(child),
                        });
                    }
                }
                "feoffset" => {
                    let dx = first_number(child.attribute("dx"));
                    let dy = first_number(child.attribute("dy"));
                    if dx.is_finite() && dy.is_finite() {
                        primitives.push(FilterPrimitive::Offset {
                            dx,
                            dy,
                            input: filter_input(child.attribute("in"), FilterInput::Previous),
                            result: filter_result(child),
                        });
                    }
                }
                "feflood" => {
                    let color = child
                        .attribute("flood-color")
                        .and_then(parse_color)
                        .unwrap_or(Color::BLACK);
                    let opacity = child
                        .attribute("flood-opacity")
                        .and_then(parse_opacity)
                        .unwrap_or(1.0);
                    primitives.push(FilterPrimitive::Flood {
                        color,
                        opacity,
                        result: filter_result(child),
                    });
                }
                "fecolormatrix" => {
                    if let Some(kind) = parse_color_matrix(child) {
                        primitives.push(FilterPrimitive::ColorMatrix {
                            kind,
                            input: filter_input(child.attribute("in"), FilterInput::Previous),
                            result: filter_result(child),
                        });
                    }
                }
                "feblend" => {
                    primitives.push(FilterPrimitive::Blend {
                        mode: parse_blend_mode(child.attribute("mode")),
                        input: filter_input(child.attribute("in"), FilterInput::Previous),
                        input2: filter_input(child.attribute("in2"), FilterInput::SourceGraphic),
                        result: filter_result(child),
                    });
                }
                "femerge" => {
                    let mut inputs = Vec::new();
                    for node in &child.children {
                        let XmlNode::Element(node) = node else {
                            continue;
                        };
                        if node.local_name_lower() == "femergenode" {
                            inputs.push(filter_input(node.attribute("in"), FilterInput::Previous));
                        }
                        if inputs.len() >= MAX_FILTER_PRIMITIVES {
                            break;
                        }
                    }
                    primitives.push(FilterPrimitive::Merge {
                        inputs,
                        result: filter_result(child),
                    });
                }
                "fecomposite" => {
                    primitives.push(FilterPrimitive::Composite {
                        operator: parse_composite_operator(child),
                        input: filter_input(child.attribute("in"), FilterInput::Previous),
                        input2: filter_input(child.attribute("in2"), FilterInput::SourceGraphic),
                        result: filter_result(child),
                    });
                }
                _ => {}
            }
        }
        specs.push(FilterSpec {
            filter_units: parse_object_box_units(element.attribute("filterUnits")),
            x: RegionLength::parse(element.attribute("x"), RegionLength::Percent(-0.1)),
            y: RegionLength::parse(element.attribute("y"), RegionLength::Percent(-0.1)),
            width: RegionLength::parse(element.attribute("width"), RegionLength::Percent(1.2)),
            height: RegionLength::parse(element.attribute("height"), RegionLength::Percent(1.2)),
            primitives,
        });
    }
    specs
}
