//! #1545: canvas 2D API — getContext/fillRect/clearRect/strokeRect and style accessors.

#![cfg(feature = "js")]

use megi_js::runtime::JsRuntime;

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

#[test]
fn canvas_rejects_utf8_color_and_overflowing_image_data() {
    // #1772/#1773: hostile script inputs must not panic the render thread.
    let mut runtime = runtime();
    let result = runtime
        .eval(
            "var c = document.createElement('canvas'); \
             c.setAttribute('width','4'); c.setAttribute('height','4'); \
             document.body.appendChild(c); \
             var ctx = c.getContext('2d'); \
             ctx.fillStyle = '#éa'; \
             ctx.fillRect(0, 0, 1, 1); \
             var bad = ctx.getImageData(1e30, 0, 1, 1); \
             String(bad === null || bad === undefined);",
            "hostile_canvas.js",
        )
        .expect("canvas script must not panic");
    assert_eq!(result, "true");
    let surfaces = runtime.canvas_surfaces();
    let surface = surfaces.values().next().expect("surface");
    // An invalid color keeps the previous (opaque black) fill style — no panic, no change.
    assert_eq!(
        &surface.pixels[0..4],
        &[0, 0, 0, 255],
        "invalid fill is a no-op"
    );
}

/// Canvas arc + fill primitives (#1959): charts and canvas-confetti rely on them.
#[test]
fn canvas_arc_and_fill_paint_pixels() {
    let mut rt = runtime();
    rt.eval(
        "var c=document.createElement('canvas'); c.width=100; c.height=100; \
         var x=c.getContext('2d'); x.fillStyle='#ff0000'; x.beginPath(); \
         x.arc(50,50,20,0,Math.PI*2); x.fill(); \
         var d=x.getImageData(50,50,1,1).data; window.__px=[d[0],d[1],d[2],d[3]].join(',');",
        "t",
    )
    .expect("arc/fill must not throw (#1959)");
    assert_eq!(rt.eval("window.__px", "t").unwrap(), "255,0,0,255");
}

/// Text primitives must exist so canvas scripts don't abort (#1961). The
/// deterministic engine has no font rasterizer in `js` (DAG), so fillText
/// paints a simple glyph box and measureText returns an approximate advance.
#[test]
fn canvas_text_primitives_exist() {
    let mut rt = runtime();
    let out = rt
        .eval(
            "var c=document.createElement('canvas'); c.width=80; c.height=40; \
             var x=c.getContext('2d'); x.fillStyle='#00ff00'; x.fillText('AB',5,20); \
             x.strokeText('AB',5,20); \
             [typeof x.fillText, typeof x.strokeText, x.measureText('AB').width>0].join('|')",
            "t",
        )
        .expect("text primitives must not throw (#1961)");
    assert_eq!(out, "function|function|true");
}

#[test]
fn canvas_global_alpha_blends_fill_rect() {
    // #1961: globalAlpha must scale source alpha for drawing operations (WHATWG canvas §4.12.5.1.12).
    let mut runtime = runtime();
    runtime
        .eval(
            "var c = document.createElement('canvas'); \
             c.setAttribute('width','2'); c.setAttribute('height','2'); \
             document.body.appendChild(c); \
             var ctx = c.getContext('2d'); \
             ctx.globalAlpha = 0.5; \
             ctx.fillStyle = '#ff0000'; \
             ctx.fillRect(0, 0, 1, 1);",
            "t",
        )
        .expect("canvas script");
    let surfaces = runtime.canvas_surfaces();
    let surface = surfaces.values().next().expect("surface");
    assert_eq!(
        &surface.pixels[0..4],
        &[255, 0, 0, 128],
        "globalAlpha=0.5 must produce ~50% alpha, got {:?}",
        &surface.pixels[0..4]
    );
}

#[test]
fn canvas_width_and_height_are_accessible_from_js() {
    // #1961: HTMLCanvasElement.width/height reflect the backing surface dimensions.
    let mut runtime = runtime();
    let out = runtime
        .eval(
            "var c = document.createElement('canvas'); \
             c.setAttribute('width','7'); c.setAttribute('height','9'); \
             document.body.appendChild(c); \
             c.getContext('2d'); \
             String(c.width + '|' + c.height);",
            "t",
        )
        .expect("canvas script");
    assert_eq!(out, "7|9");
}

#[test]
fn canvas_global_alpha_getter_roundtrips() {
    let mut runtime = runtime();
    let out = runtime
        .eval(
            "var c = document.createElement('canvas'); document.body.appendChild(c); \
             var ctx = c.getContext('2d'); \
             var initial = String(ctx.globalAlpha); \
             ctx.globalAlpha = 0.25; \
             initial + '|' + String(ctx.globalAlpha);",
            "t",
        )
        .expect("canvas script");
    assert_eq!(out, "1|0.25");
}

#[test]
fn canvas_width_setter_reflects_and_sizes_backing_surface() {
    // #1961: setting width/height from JS updates both the attribute and the backing store.
    let mut runtime = runtime();
    let out = runtime
        .eval(
            "var c = document.createElement('canvas'); document.body.appendChild(c); \
             c.width = 6; c.height = 4; \
             var ctx = c.getContext('2d'); \
             String(c.width) + '|' + String(c.height) + '|' + String(c.getAttribute('width'));",
            "t",
        )
        .expect("canvas script");
    assert_eq!(out, "6|4|6");
    let surfaces = runtime.canvas_surfaces();
    let surface = surfaces.values().next().expect("surface");
    assert_eq!((surface.width, surface.height), (6, 4));
}

#[test]
fn fill_arc_with_huge_radius_clips_to_surface() {
    // #2154: fill() on an arc with a huge radius must clip to the canvas surface.
    let mut runtime = runtime();
    let out = runtime
        .eval(
            "var c = document.createElement('canvas'); c.width = 2; c.height = 2; \
             document.body.appendChild(c); \
             var ctx = c.getContext('2d'); \
             ctx.fillStyle = '#ff0000'; \
             ctx.beginPath(); ctx.arc(0, 0, 1e12, 0, Math.PI * 2); ctx.fill(); \
             var d = ctx.getImageData(0, 0, 1, 1).data; \
             String(d[0]) + ',' + String(d[3]);",
            "t",
        )
        .expect("canvas script");
    assert_eq!(out, "255,255");
}

#[test]
fn put_image_data_copies_typed_array_bytes_in_linear_time() {
    // #2183: `putImageData` on a 512x512 Uint8ClampedArray previously ran >1M
    // JS string property lookups (~1.7 s+ on desktop, ~27 s on audit runner). Copying
    // directly from the backing ArrayBuffer reduces it to a few milliseconds, and
    // must also honor subarray byteOffset.
    let mut runtime = runtime();
    let start = std::time::Instant::now();
    let out = runtime
        .eval(
            "var c = document.createElement('canvas'); c.width = 512; c.height = 512; \
             document.body.appendChild(c); \
             var ctx = c.getContext('2d'); \
             var buf = new Uint8ClampedArray(512 * 512 * 4); \
             buf[0] = 12; buf[1] = 34; buf[2] = 56; buf[3] = 255; \
             var last = buf.length - 4; \
             buf[last] = 200; buf[last + 1] = 150; buf[last + 2] = 100; buf[last + 3] = 250; \
             ctx.putImageData({ width: 512, height: 512, data: buf }, 0, 0); \
             var sub = new Uint8Array([99, 1, 2, 3, 4, 88]).subarray(1, 5); \
             ctx.putImageData({ width: 1, height: 1, data: sub }, 10, 10); \
             var d0 = ctx.getImageData(0, 0, 1, 1).data; \
             var d1 = ctx.getImageData(511, 511, 1, 1).data; \
             var d2 = ctx.getImageData(10, 10, 1, 1).data; \
             [d0[0],d0[1],d0[2],d0[3], d1[0],d1[1],d1[2],d1[3], d2[0],d2[1],d2[2],d2[3]].join(',');",
            "put_image_data_fast",
        )
        .expect("putImageData");
    let elapsed = start.elapsed();
    assert_eq!(out, "12,34,56,255,200,150,100,250,1,2,3,4");
    assert!(
        elapsed < std::time::Duration::from_millis(250),
        "512x512 putImageData took {elapsed:?} (expected linear ArrayBuffer copy < 250ms)"
    );
}
