# ADR-007 — DPR raster boundary: CSS geometry in, physical pixels out

**Date:** 2026-11-25 · **Status:** accepted · **Scope:** `display`, `render`, `browser` (no new crate edge)

## Context

On a HiDPI monitor the window reports a winit `scale_factor` (e.g. `1.25`): a
`1600×900` physical window has a `1280×720` CSS viewport. Until this ADR the
pipeline laid out, painted **and rasterized** at CSS resolution and the shell
upscaled the CSS frame with nearest-neighbour sampling in `present.rs`. Page
typography was therefore rasterized at `1280×720` and blurred on presentation,
visibly worse than a headless render of the same document.

`device_pixel_ratio` was already forwarded to JavaScript (`window.devicePixelRatio`)
and to responsive image selection (`srcset`), but not to the raster pass.

## Decision

1. **Explicit channel.** `megi_render::RenderOptions::raster_scale: f32` and
   `megi_browser::worker::RenderRequest::raster_scale: f32` carry physical device
   pixels per CSS pixel (default `1.0`). They are separate from:
   - the CSS viewport (`Viewport` / `RenderRequest::css_width/height`), which
     drives layout, media queries, `srcset` and JS geometry;
   - page zoom, which changes the CSS viewport and is applied again by the
     present layer. `WindowApp::raster_scale()` uses the winit *device* scale
     only; headless renders keep `1.0`.

2. **Owned transform in `display`.** `megi_display::RasterScale` (`Copy`, no
   lifetimes, no `Rc`/`Weak`, no `unsafe`) converts CSS geometry to raster
   geometry with `apply(&DisplayList) -> DisplayList`: rectangles, border widths,
   font sizes, baselines, advances, radii, box/text-shadow offsets, blur/spread,
   filter lengths, gradient lengths and nested `OpacityGroup`/`EffectGroup`/
   text-mask items. Percentages and dimensionless filter values are preserved;
   identity returns a plain clone. `display` already owns the display-list
   contract shared by `paint` and `raster`, so it is the only layer that must
   understand every primitive — `render` does not grow a second, partial
   variant-matching switch and `paint` continues to emit CSS coordinates.

3. **Raster stage.** `render::finish_render` scales a *copy* of the display list
   only when `raster_scale != 1`, rasterizes at
   `round(css_width × scale) × round(css_height × scale)` (bounded by
   `MAX_VIEWPORT_AREA`) and keeps `RenderOutput::display_list` and
   `RenderOutput::hit_regions` in CSS pixels. Scale `1.0` keeps the exact old
   borrow and byte-identical golden PNGs.

4. **Presentation.** `browser` frames are physical-resolution bitmaps
   (`RenderFrame::width/height`); `content_height`, link/form/find rectangles and
   pointer coordinates remain CSS. `present_region_at_scaled` /
   `present_region_with_insets_scaled` convert CSS scroll rows to raster rows and
   derive source columns from the frame width: a physical frame in a same-sized
   window buffer is a 1:1 blit. Pointer hit-testing still divides by the
   effective scale (`device × zoom`) in `WindowApp::page_coordinates`, so clicks
   land on the same CSS hit regions. The process-renderer path transports the
   scale inside the existing `IpcRenderOptions` (`#[serde(default)]` keeps wire
   compatibility) and reconstructs frame dimensions from the decoded PNG.

## Consequences

- No crate edge changes: `display → raster → render → webview → browser` is
  unchanged; `display` only uses its existing `core`/`css`/`image` dependencies.
  `docs/ARCHITECTURE.md` needs no DAG update, only this pointer.
- `scale = 1` is a strict no-op (byte-identical PNGs and frames); all existing
  callers, goldens and headless screenshots are unaffected.
- Hit-testing, scrolling, form editing and find overlays remain in CSS
  coordinates and never need a scale conversion at their call sites.
- Physical area follows the existing 16 Mi-pixel cap, so a large CSS viewport
  at a large scale is rejected rather than allocating an unbounded framebuffer.

## Known limitations

- Presentation still uses nearest-neighbour sampling for the residual
  mismatch when the window buffer is not exactly the frame size (page zoom,
  fractional scale rounding, chrome insets); bilinear resampling is out of
  scope.
- The rasterizer clamps blur kernels at `MAX_SHADOW_BLUR` **after** scaling, so
  a CSS blur at the cap reaches `64 / scale` CSS px of support at high DPR.
- The height probe compares CSS heights (not pixmap heights) and therefore
  renders one extra pass exactly as before; no attempt is made to reuse the
  probe pixmap as the final high-DPI frame.
