//! #1545: canvas 2D API — getContext/fillRect/clearRect/strokeRect and style accessors.

#![cfg(feature = "js")]

use js::runtime::JsRuntime;

fn runtime() -> JsRuntime {
    JsRuntime::new().expect("runtime")
}

#[test]
fn canvas_get_context_and_fill_rect() {
    let mut runtime = runtime();
    runtime
        .eval(
            "var c = document.createElement('canvas'); \
             c.setAttribute('width','4'); c.setAttribute('height','4'); \
             document.body.appendChild(c); \
             var ctx = c.getContext('2d'); \
             ctx.fillStyle = '#ff0000'; \
             ctx.fillRect(0, 0, 2, 2); \
             globalThis.__canvasNode = c;",
            "t",
        )
        .expect("canvas script");
    let surfaces = runtime.canvas_surfaces();
    assert_eq!(surfaces.len(), 1, "one canvas surface created");
    let surface = surfaces.values().next().expect("surface");
    assert_eq!((surface.width, surface.height), (4, 4));
    assert_eq!(
        &surface.pixels[0..4],
        &[255, 0, 0, 255],
        "fillRect top-left"
    );
    assert_eq!(&surface.pixels[8..12], &[0, 0, 0, 0], "outside untouched");
}

#[test]
fn canvas_styles_and_non_2d_context() {
    let mut runtime = runtime();
    let out = runtime
        .eval(
            "var c = document.createElement('canvas'); document.body.appendChild(c); \
             var ctx = c.getContext('2d'); \
             ctx.fillStyle = 'rgb(0, 255, 0)'; \
             var before = ctx.fillStyle; \
             ctx.strokeStyle = 'blue'; \
             var noCtx = c.getContext('webgl'); \
             String(before + '|' + ctx.strokeStyle + '|' + (noCtx === null));",
            "t",
        )
        .expect("styles");
    assert_eq!(out, "#00ff00|#0000ff|true");
}

#[test]
fn canvas_clear_rect_removes_pixels() {
    let mut runtime = runtime();
    runtime
        .eval(
            "var c = document.createElement('canvas'); \
             c.setAttribute('width','2'); c.setAttribute('height','2'); \
             document.body.appendChild(c); \
             var ctx = c.getContext('2d'); \
             ctx.fillStyle = '#ffffff'; ctx.fillRect(0,0,2,2); \
             ctx.clearRect(0,0,1,1);",
            "t",
        )
        .expect("clear");
    let surfaces = runtime.canvas_surfaces();
    let surface = surfaces.values().next().expect("surface");
    assert_eq!(&surface.pixels[0..4], &[0, 0, 0, 0], "cleared pixel");
    assert_eq!(
        &surface.pixels[12..16],
        &[255, 255, 255, 255],
        "other pixel kept"
    );
}

#[test]
fn oversized_fill_rect_is_clipped_to_the_surface() {
    // #1618: fillRect(5000x5000) on a 300x150 canvas must touch 45k pixels, not 25M.
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime
        .eval(
            "var c = document.createElement('canvas'); c.width = 300; c.height = 150; \
             document.body.appendChild(c); \
             var ctx = c.getContext('2d'); ctx.fillRect(0, 0, 5000, 5000);",
            "t",
        )
        .expect("fillRect");
    let surfaces = runtime.canvas_surfaces();
    let surface = surfaces.values().next().expect("surface");
    assert_eq!(
        surface.operations,
        300 * 150,
        "oversized rect must be clipped to the surface (did {} blends)",
        surface.operations
    );
}
