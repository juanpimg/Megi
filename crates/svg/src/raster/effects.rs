//! Submodule of [`super`]: clip paths, masks and the filter chain.

use super::*;

/// Maps a reference definition's local geometry to device pixels.
fn effect_point(effect: &EffectRef, viewport: Transform, x: f32, y: f32) -> (f32, f32) {
    let (x, y) = if effect.units == Units::ObjectBoundingBox {
        (
            effect.bbox.0 + x * effect.bbox.2,
            effect.bbox.1 + y * effect.bbox.3,
        )
    } else {
        (x, y)
    };
    let (x, y) = effect.transform.apply(x, y);
    viewport.apply(x, y)
}

/// Multiplies the layer alpha by the clip coverage of a `<clipPath>` (intersection of all
/// its child shapes, each with its own fill rule).
pub(crate) fn apply_clip(
    layer: &mut Canvas,
    document: &Document,
    effect: &EffectRef,
    viewport: Transform,
) -> Result<(), SvgError> {
    let Some(spec): Option<&ClipPathSpec> = document.clip_paths.get(effect.index as usize) else {
        return Ok(());
    };
    let mut coverage = vec![0.0f32; layer.width * layer.height];
    for shape in &spec.shapes {
        let mut device = Vec::with_capacity(shape.subpaths.len());
        for subpath in &shape.subpaths {
            let points: Vec<(f32, f32)> = subpath
                .points
                .iter()
                .map(|(x, y)| effect_point(effect, viewport, *x, *y))
                .filter(|(x, y)| x.is_finite() && y.is_finite())
                .collect();
            if points.len() >= 2 {
                device.push(SubPath {
                    points,
                    closed: subpath.closed,
                });
            }
        }
        fill_scanline(
            &device,
            spec.fill_rule,
            layer.width,
            layer.height,
            &mut coverage,
        );
    }
    for (index, alpha) in layer.pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let coverage = coverage[index].clamp(0.0, 1.0);
        let value = f32::from(alpha[3]) * coverage;
        alpha[3] = value.clamp(0.0, 255.0).round() as u8;
    }
    Ok(())
}

/// Multiplies the layer alpha by the luminance of a `<mask>` content rendering.
pub(crate) fn apply_mask(
    layer: &mut Canvas,
    document: &Document,
    effect: &EffectRef,
    context: &PaintContext<'_>,
) -> Result<(), SvgError> {
    let Some(spec): Option<&MaskSpec> = document.masks.get(effect.index as usize) else {
        return Ok(());
    };
    let viewport = context.viewport;
    let bbox = effect.bbox;
    let content_transform = match spec.content_units {
        Units::UserSpaceOnUse => Transform::IDENTITY,
        Units::ObjectBoundingBox => {
            let sx = if bbox.2.abs() > f32::EPSILON {
                bbox.2
            } else {
                1.0
            };
            let sy = if bbox.3.abs() > f32::EPSILON {
                bbox.3
            } else {
                1.0
            };
            Transform::translate(bbox.0, bbox.1).then(Transform::scale(sx, sy))
        }
    };
    let mask_viewport = viewport.then(effect.transform).then(content_transform);
    let mut content = Canvas::new(layer.width, layer.height);
    for shape in &spec.shapes {
        let shape_context = PaintContext {
            gradients: context.gradients,
            patterns: context.patterns,
            shape_transform: shape.transform,
            bbox: shape.bbox,
            viewport: mask_viewport,
            depth: context.depth,
            budget: context.budget,
        };
        draw_shape(&mut content, shape, &shape_context)?;
    }
    // Reference region in the element's user space, then device pixels.
    let region = mask_region_pixels(
        layer,
        effect,
        viewport,
        &spec.x,
        &spec.y,
        &spec.width,
        &spec.height,
    )?;
    for (index, alpha) in layer.pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let x = (index % layer.width) as f32 + 0.5;
        let y = (index / layer.width) as f32 + 0.5;
        let inside = region.is_some_and(|(x0, y0, x1, y1)| x >= x0 && x < x1 && y >= y0 && y < y1);
        if !inside {
            alpha[3] = 0;
            continue;
        }
        let offset = index * 4;
        let mask_alpha = f32::from(content.pixels[offset + 3]) / 255.0;
        let luminance = (0.2126 * f32::from(content.pixels[offset])
            + 0.7152 * f32::from(content.pixels[offset + 1])
            + 0.0722 * f32::from(content.pixels[offset + 2]))
            / 255.0;
        let coverage = (mask_alpha * luminance).clamp(0.0, 1.0);
        alpha[3] = (f32::from(alpha[3]) * coverage).clamp(0.0, 255.0).round() as u8;
    }
    Ok(())
}

/// Computes the device-space rectangle of a mask/filter region.
fn mask_region_pixels(
    layer: &Canvas,
    effect: &EffectRef,
    viewport: Transform,
    x: &crate::document::RegionLength,
    y: &crate::document::RegionLength,
    width: &crate::document::RegionLength,
    height: &crate::document::RegionLength,
) -> Result<Option<(f32, f32, f32, f32)>, SvgError> {
    let bbox = effect.bbox;
    let (x, y, w, h) = if effect.units == Units::ObjectBoundingBox {
        // In objectBoundingBox units plain numbers are fractions of the reference box.
        let fraction = |value: &crate::document::RegionLength, base: f32| -> f32 {
            match value {
                crate::document::RegionLength::Number(number) => number * base,
                crate::document::RegionLength::Percent(percent) => percent * base,
            }
        };
        (
            bbox.0 + fraction(x, bbox.2),
            bbox.1 + fraction(y, bbox.3),
            fraction(width, bbox.2),
            fraction(height, bbox.3),
        )
    } else {
        (
            x.resolve(bbox.2),
            y.resolve(bbox.3),
            width.resolve(bbox.2),
            height.resolve(bbox.3),
        )
    };
    if !x.is_finite() || !y.is_finite() || !w.is_finite() || !h.is_finite() || w <= 0.0 || h <= 0.0
    {
        return Ok(None);
    }
    let map = |px: f32, py: f32| {
        let (rx, ry) = effect.transform.apply(px, py);
        viewport.apply(rx, ry)
    };
    let corners = [map(x, y), map(x + w, y), map(x, y + h), map(x + w, y + h)];
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for (px, py) in corners {
        if px.is_finite() && py.is_finite() {
            min_x = min_x.min(px);
            min_y = min_y.min(py);
            max_x = max_x.max(px);
            max_y = max_y.max(py);
        }
    }
    if !min_x.is_finite() || !min_y.is_finite() {
        return Ok(None);
    }
    let _ = layer;
    Ok(Some((
        min_x.clamp(0.0, layer.width as f32),
        min_y.clamp(0.0, layer.height as f32),
        max_x.clamp(0.0, layer.width as f32),
        max_y.clamp(0.0, layer.height as f32),
    )))
}

/// Worst-case scratch bytes for a filter chain: 4 base buffers plus at most two
/// full-layer buffers per primitive (#430).
pub(crate) fn filter_scratch_bytes(layer_bytes: u64, primitives: usize) -> u64 {
    layer_bytes.saturating_mul(4u64.saturating_add(2u64.saturating_mul(primitives as u64)))
}

/// Applies a supported filter chain to the layer pixels.
///
/// Primitives are resolved in order with `in`/`in2`/`result` references
/// (`SourceGraphic`, `SourceAlpha`, `Previous` or a named result); the last primitive's
/// output is the filter result. The filter region clips the final result.
pub(crate) fn apply_filter(
    layer: &mut Canvas,
    spec: &FilterSpec,
    effect: &EffectRef,
    viewport: Transform,
) -> Result<(), SvgError> {
    if spec.primitives.is_empty() {
        return Ok(());
    }
    let region = mask_region_pixels(
        layer,
        effect,
        viewport,
        &spec.x,
        &spec.y,
        &spec.width,
        &spec.height,
    )?;
    let transform = viewport.then(effect.transform);
    let scale = (transform.a * transform.d - transform.b * transform.c)
        .abs()
        .sqrt();
    if !scale.is_finite() || scale <= 0.0 {
        return Ok(());
    }
    // Bound worst-case scratch before cloning (#430).
    let layer_bytes = layer.pixels.len() as u64;
    let worst_case = filter_scratch_bytes(layer_bytes, spec.primitives.len());
    if worst_case > MAX_FILTER_SCRATCH_BYTES {
        return Err(SvgError::Limit(format!(
            "SVG filter scratch budget of {MAX_FILTER_SCRATCH_BYTES} bytes exceeded"
        )));
    }
    let source = layer.pixels.clone();
    let mut source_alpha = source.clone();
    for pixel in source_alpha.as_chunks_mut::<4>().0 {
        pixel[0] = 0;
        pixel[1] = 0;
        pixel[2] = 0;
    }
    let mut results: std::collections::HashMap<String, Vec<u8>> = std::collections::HashMap::new();
    let mut previous = source.clone();
    // Filter Effects: a reference to a result that does not exist (typo, forward reference)
    // resolves to transparent black, not to the source graphic.
    let transparent = vec![0u8; source.len()];
    for primitive in &spec.primitives {
        let input = |reference: &FilterInput| -> Vec<u8> {
            match reference {
                FilterInput::SourceGraphic => source.clone(),
                FilterInput::SourceAlpha => source_alpha.clone(),
                FilterInput::Previous => previous.clone(),
                FilterInput::Named(name) => results
                    .get(name)
                    .cloned()
                    .unwrap_or_else(|| transparent.clone()),
            }
        };
        let (pixels, result) = match primitive {
            FilterPrimitive::GaussianBlur {
                std_deviation,
                input: reference,
                result,
            } => {
                let mut pixels = input(reference);
                box_blur(
                    &mut pixels,
                    layer.width,
                    layer.height,
                    (*std_deviation * scale).max(0.0),
                );
                (pixels, result)
            }
            FilterPrimitive::Offset {
                dx,
                dy,
                input: reference,
                result,
            } => {
                let mut pixels = input(reference);
                offset_pixels(
                    &mut pixels,
                    layer.width,
                    layer.height,
                    (dx * scale).round().clamp(i32::MIN as f32, i32::MAX as f32) as i32,
                    (dy * scale).round().clamp(i32::MIN as f32, i32::MAX as f32) as i32,
                );
                (pixels, result)
            }
            FilterPrimitive::Flood {
                color,
                opacity,
                result,
            } => {
                let alpha = (f32::from(color.a) / 255.0 * opacity).clamp(0.0, 1.0);
                let mut pixels = vec![0u8; layer.pixels.len()];
                for pixel in pixels.as_chunks_mut::<4>().0 {
                    pixel[0] = color.r;
                    pixel[1] = color.g;
                    pixel[2] = color.b;
                    pixel[3] = (alpha * 255.0).round() as u8;
                }
                (pixels, result)
            }
            FilterPrimitive::ColorMatrix {
                kind,
                input: reference,
                result,
            } => (apply_color_matrix(&input(reference), kind), result),
            FilterPrimitive::Blend {
                mode,
                input: reference,
                input2,
                result,
            } => {
                let src = input(reference);
                let dst = input(input2);
                (blend_images(&src, &dst, *mode), result)
            }
            FilterPrimitive::Merge { inputs, result } => {
                let mut pixels = vec![0u8; layer.pixels.len()];
                for reference in inputs {
                    pixels = composite_over(&input(reference), &pixels);
                }
                (pixels, result)
            }
            FilterPrimitive::Composite {
                operator,
                input: reference,
                input2,
                result,
            } => {
                let src = input(reference);
                let dst = input(input2);
                (composite_images_ext(&src, &dst, operator), result)
            }
        };
        previous = pixels.clone();
        if let Some(name) = result {
            results.insert(name.clone(), pixels.clone());
        }
    }
    let mut output = previous;
    if let Some((x0, y0, x1, y1)) = region {
        for y in 0..layer.height {
            for x in 0..layer.width {
                let inside = (x as f32 + 0.5) >= x0
                    && (x as f32 + 0.5) < x1
                    && (y as f32 + 0.5) >= y0
                    && (y as f32 + 0.5) < y1;
                if !inside {
                    let offset = (y * layer.width + x) * 4;
                    output[offset + 3] = 0;
                }
            }
        }
    }
    layer.pixels = output;
    Ok(())
}
