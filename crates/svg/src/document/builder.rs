//! Submodule of [`super`]: builder extracted from the document model.

use super::*;

impl<'a> Builder<'a> {
    /// Resolves the element's own `clip-path`/`mask`/`filter` references into effect refs.
    ///
    /// `group` selects group semantics for `objectBoundingBox` units: the reference box is
    /// the descendants' union (assigned after traversal) and the transform is identity,
    /// while shapes use their local bbox and own transform.
    pub(crate) fn own_effects(
        &self,
        style: &Style,
        transform: Transform,
        bbox: Option<(f32, f32, f32, f32)>,
        group: bool,
    ) -> (Vec<EffectRef>, Vec<EffectRef>, Option<EffectRef>) {
        let reference = |units: Units, transform: Transform| {
            if group && units == Units::ObjectBoundingBox {
                (units, Transform::IDENTITY, (0.0, 0.0, 0.0, 0.0))
            } else {
                (units, transform, bbox.unwrap_or((0.0, 0.0, 0.0, 0.0)))
            }
        };
        let clip = style
            .clip_path
            .as_ref()
            .and_then(|id| self.clip_ids.get(id))
            .map(|&index| {
                let units = self.clip_paths[index as usize].units;
                let (units, transform, bbox) = reference(units, transform);
                EffectRef {
                    index,
                    units,
                    bbox,
                    transform,
                }
            });
        let mask = style
            .mask
            .as_ref()
            .and_then(|id| self.mask_ids.get(id))
            .map(|&index| {
                let units = self.masks[index as usize].mask_units;
                let (units, transform, bbox) = reference(units, transform);
                EffectRef {
                    index,
                    units,
                    bbox,
                    transform,
                }
            });
        let filter = style
            .filter
            .as_ref()
            .and_then(|id| self.filter_ids.get(id))
            .map(|&index| {
                let units = self.filters[index as usize].filter_units;
                let (units, transform, bbox) = reference(units, transform);
                EffectRef {
                    index,
                    units,
                    bbox,
                    transform,
                }
            });
        (
            clip.into_iter().collect(),
            mask.into_iter().collect(),
            filter,
        )
    }

    pub(crate) fn push_effects(
        &mut self,
        clips: &[EffectRef],
        masks: &[EffectRef],
        filter: Option<&EffectRef>,
    ) -> (usize, usize, Option<EffectRef>) {
        let previous = (
            self.active_clips.len(),
            self.active_masks.len(),
            self.active_filter.take(),
        );
        self.active_clips.extend_from_slice(clips);
        self.active_masks.extend_from_slice(masks);
        if let Some(filter) = filter {
            self.active_filter = Some(filter.clone());
        }
        previous
    }

    pub(crate) fn pop_effects(&mut self, previous: (usize, usize, Option<EffectRef>)) {
        self.active_clips.truncate(previous.0);
        self.active_masks.truncate(previous.1);
        self.active_filter = previous.2;
    }

    /// Folds the most recently emitted shape into every open group's running bbox.
    /// Called once from `push_shape`, so a leaf shape is visited once instead of
    /// once per ancestor group (#442).
    pub(crate) fn record_shape_bbox(&mut self) {
        if self.group_bbox_stack.iter().all(|frame| !frame.active) {
            return;
        }
        let Some(shape) = self.shapes.last() else {
            return;
        };
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for subpath in &shape.subpaths {
            for point in &subpath.points {
                if point.0.is_finite() && point.1.is_finite() {
                    min_x = min_x.min(point.0);
                    min_y = min_y.min(point.1);
                    max_x = max_x.max(point.0);
                    max_y = max_y.max(point.1);
                }
            }
        }
        if !min_x.is_finite() || !min_y.is_finite() {
            return;
        }
        for frame in &mut self.group_bbox_stack {
            if !frame.active {
                continue;
            }
            frame.bbox = Some(match frame.bbox {
                None => (min_x, min_y, max_x - min_x, max_y - min_y),
                Some((x, y, w, h)) => (
                    x.min(min_x),
                    y.min(min_y),
                    (x + w).max(max_x) - x.min(min_x),
                    (y + h).max(max_y) - y.min(min_y),
                ),
            });
        }
    }

    /// Copies the group's descendant bbox (root user space) into its
    /// `objectBoundingBox` effect refs. Group effects are inherited by all
    /// descendants, so every matching ref is updated. `bbox` is the incremental
    /// accumulator built while the group was open; the scan is only a fallback
    /// for shapes inserted through a path that does not call `record_shape_bbox`.
    pub(crate) fn apply_group_bbox(
        &mut self,
        start: usize,
        clips: &[EffectRef],
        masks: &[EffectRef],
        filter: Option<&EffectRef>,
        bbox: Option<(f32, f32, f32, f32)>,
    ) {
        // Plain groups used to rescan their entire subtree for nothing (#442).
        if clips.is_empty() && masks.is_empty() && filter.is_none() {
            return;
        }
        let bbox = bbox.or_else(|| {
            let mut min_x = f32::INFINITY;
            let mut min_y = f32::INFINITY;
            let mut max_x = f32::NEG_INFINITY;
            let mut max_y = f32::NEG_INFINITY;
            for shape in &self.shapes[start..] {
                for subpath in &shape.subpaths {
                    for point in &subpath.points {
                        if point.0.is_finite() && point.1.is_finite() {
                            min_x = min_x.min(point.0);
                            min_y = min_y.min(point.1);
                            max_x = max_x.max(point.0);
                            max_y = max_y.max(point.1);
                        }
                    }
                }
            }
            (min_x.is_finite() && min_y.is_finite()).then(|| {
                (
                    min_x,
                    min_y,
                    (max_x - min_x).max(0.0),
                    (max_y - min_y).max(0.0),
                )
            })
        });
        let Some(bbox) = bbox else {
            return;
        };
        let matches = |reference: &EffectRef, candidate: &EffectRef| {
            reference.index == candidate.index && reference.transform == candidate.transform
        };
        for shape in &mut self.shapes[start..] {
            for effect in &mut shape.clip {
                if clips.iter().any(|reference| matches(reference, effect)) {
                    effect.bbox = bbox;
                }
            }
            for effect in &mut shape.mask {
                if masks.iter().any(|reference| matches(reference, effect)) {
                    effect.bbox = bbox;
                }
            }
            if let (Some(effect), Some(reference)) = (shape.filter.as_mut(), filter)
                && matches(reference, effect)
            {
                effect.bbox = bbox;
            }
        }
    }
}
