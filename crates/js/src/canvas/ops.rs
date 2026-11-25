//! Narrow context-data interface between Canvas 2D bindings and realm state.
//!
//! The canvas bindings used to borrow `crate::host::state::Host` directly from
//! every native callback. That put them inside the `events → host → runtime →
//! events` strongly connected component: the runtime installs the bindings,
//! and every binding reached back into the host that owns nested realms.
//!
//! This module is the *lower* layer both sides now depend on. It stores a small
//! set of non-capturing function pointers in Boa's `HostDefined` data. The
//! runtime installs the implementations (which may borrow the host) once, when
//! globals are created; the bindings call only these pointers and never name
//! `Host`, `host_cell` or `JsRuntime`. `CanvasOps` is `Copy` and contains no
//! borrows, so a binding can copy it out of the context before mutating the
//! context again.
//!
//! Spec: WHATWG HTML §4.12.5.1 (Canvas 2D drawing state).

use boa_engine::Context;
use megi_core::NodeId;

use crate::canvas::CanvasSurface;

/// Resolves a JS side-table handle to a live DOM node.
pub(crate) type ResolveHandleFn = fn(&mut Context, u32) -> Option<NodeId>;
/// Reads `<canvas width>`/`<canvas height>` (already capped by the implementation).
pub(crate) type DimensionsFn = fn(&mut Context, NodeId) -> Option<(u32, u32)>;
/// Creates the surface on first use; `false` means the size cap was exceeded.
pub(crate) type EnsureSurfaceFn = fn(&mut Context, NodeId, u32, u32) -> bool;
/// Allocates (or reuses) a side-table handle for a DOM node.
pub(crate) type HandleForFn = fn(&mut Context, NodeId) -> Option<u32>;
/// Runs `f` with the surface for `node`, if one exists.
pub(crate) type WithSurfaceFn = fn(&mut Context, NodeId, &mut dyn FnMut(&mut CanvasSurface));
/// Clones one surface for `drawImage`.
pub(crate) type SnapshotFn = fn(&mut Context, NodeId) -> Option<CanvasSurface>;

/// The whole capability surface the Canvas 2D bindings need from a realm.
#[derive(Clone, Copy)]
pub(crate) struct CanvasOps {
    pub(crate) resolve_handle: ResolveHandleFn,
    pub(crate) dimensions: DimensionsFn,
    pub(crate) ensure_surface: EnsureSurfaceFn,
    pub(crate) handle_for: HandleForFn,
    pub(crate) with_surface: WithSurfaceFn,
    pub(crate) snapshot: SnapshotFn,
}

impl CanvasOps {
    /// Installs the capability slot in `context` (once, during realm setup).
    pub(crate) fn install(context: &mut Context, ops: Self) {
        let _ = context.insert_data(ops);
    }

    /// Copies the installed capabilities out of `context`.
    pub(crate) fn from_context(context: &Context) -> Option<Self> {
        context.get_data::<Self>().copied()
    }

    /// Resolves a wrapper handle to a live node, without naming `Host`.
    pub(crate) fn resolve_handle(self, context: &mut Context, handle: u32) -> Option<NodeId> {
        (self.resolve_handle)(context, handle)
    }

    /// Reads the capped `<canvas>` dimensions.
    pub(crate) fn dimensions(self, context: &mut Context, node: NodeId) -> Option<(u32, u32)> {
        (self.dimensions)(context, node)
    }

    /// Creates the surface on first use; `false` means the size cap was exceeded.
    pub(crate) fn ensure_surface(
        self,
        context: &mut Context,
        node: NodeId,
        width: u32,
        height: u32,
    ) -> bool {
        (self.ensure_surface)(context, node, width, height)
    }

    /// Allocates or reuses a side-table handle for `node`.
    pub(crate) fn handle_for(self, context: &mut Context, node: NodeId) -> Option<u32> {
        (self.handle_for)(context, node)
    }

    /// Runs `f` on the surface for `node`, if it exists, and returns its result.
    pub(crate) fn with_surface<R>(
        self,
        context: &mut Context,
        node: NodeId,
        f: impl FnOnce(&mut CanvasSurface) -> R,
    ) -> Option<R> {
        let mut result = None;
        let mut f = Some(f);
        (self.with_surface)(context, node, &mut |surface| {
            if let Some(f) = f.take() {
                result = Some(f(surface));
            }
        });
        result
    }

    /// Reads a value from a surface without exposing mutable access to callers.
    pub(crate) fn read_surface<R>(
        self,
        context: &mut Context,
        node: NodeId,
        f: impl FnOnce(&CanvasSurface) -> R,
    ) -> Option<R> {
        self.with_surface(context, node, |surface| f(surface))
    }

    /// Clones one surface snapshot for `drawImage`.
    pub(crate) fn snapshot(self, context: &mut Context, node: NodeId) -> Option<CanvasSurface> {
        (self.snapshot)(context, node)
    }
}

/// Runs `f` on a surface when a realm has canvas capabilities installed.
pub(crate) fn with_surface(
    context: &mut Context,
    node: NodeId,
    f: impl FnOnce(&mut CanvasSurface),
) {
    if let Some(ops) = CanvasOps::from_context(context) {
        let _ = ops.with_surface(context, node, f);
    }
}

/// Reads a surface value when a realm has canvas capabilities installed.
pub(crate) fn read_surface<R>(
    context: &mut Context,
    node: NodeId,
    f: impl FnOnce(&CanvasSurface) -> R,
) -> Option<R> {
    CanvasOps::from_context(context)?.read_surface(context, node, f)
}

/// Runs `f` on a surface and returns its value.
pub(crate) fn surface_value<R>(
    context: &mut Context,
    node: NodeId,
    f: impl FnOnce(&mut CanvasSurface) -> R,
) -> Option<R> {
    CanvasOps::from_context(context)?.with_surface(context, node, f)
}

/// Resolves a side-table handle through the installed capabilities.
pub(crate) fn resolve_handle(context: &mut Context, handle: u32) -> Option<NodeId> {
    CanvasOps::from_context(context)?.resolve_handle(context, handle)
}

/// Reads capped `<canvas>` dimensions through the installed capabilities.
pub(crate) fn dimensions(context: &mut Context, node: NodeId) -> Option<(u32, u32)> {
    CanvasOps::from_context(context)?.dimensions(context, node)
}

/// Creates a canvas surface on first use through the installed capabilities.
pub(crate) fn ensure_surface(context: &mut Context, node: NodeId, width: u32, height: u32) -> bool {
    CanvasOps::from_context(context)
        .is_some_and(|ops| ops.ensure_surface(context, node, width, height))
}

/// Allocates or reuses a side-table handle through the installed capabilities.
pub(crate) fn handle_for(context: &mut Context, node: NodeId) -> Option<u32> {
    CanvasOps::from_context(context)?.handle_for(context, node)
}

/// Clones a surface snapshot through the installed capabilities.
pub(crate) fn snapshot(context: &mut Context, node: NodeId) -> Option<CanvasSurface> {
    CanvasOps::from_context(context)?.snapshot(context, node)
}

#[cfg(test)]
mod tests {
    use super::*;
    use megi_core::Document;

    #[test]
    fn canvas_ops_slot_round_trip() {
        // The slot must be Copy, readable from a shared context, and its function
        // pointers must remain callable after the context is mutated again.
        let mut context = Context::default();
        let node = Document::new().create_element(megi_core::Namespace::Html, "canvas", vec![]);
        let raw = node.get();
        CanvasOps::install(
            &mut context,
            CanvasOps {
                resolve_handle: |_context, handle| NodeId::from_raw(handle),
                dimensions: |_context, _node| Some((7, 9)),
                ensure_surface: |_context, _node, _w, _h| true,
                handle_for: |_context, _node| Some(3),
                with_surface: |_context, _node, _f| {},
                snapshot: |_context, _node| None,
            },
        );
        let ops = CanvasOps::from_context(&context).expect("installed canvas ops");
        let copied = ops;
        assert_eq!(ops.resolve_handle(&mut context, raw), Some(node));
        assert_eq!(copied.dimensions(&mut context, node), Some((7, 9)));
        assert_eq!(copied.handle_for(&mut context, node), Some(3));
    }
}
