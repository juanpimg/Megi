//! M6 software WebGL foundation: clearColor/clear/readPixels over CanvasSurface.
//!
//! Spec: WebGL 1.0 §5.14 (clearing the buffers), §6.2 (reading pixels).

#![cfg(feature = "js")]

use megi_js::runtime::JsRuntime;

fn runtime() -> JsRuntime {
    JsRuntime::new().expect("runtime")
}

#[test]
fn webgl_clear_color_and_read_pixels() {
    let mut rt = runtime();
    let out = rt
        .eval(
            "var c = document.createElement('canvas'); c.width = 4; c.height = 4; \
             document.body.appendChild(c); \
             var gl = c.getContext('webgl'); \
             gl.clearColor(1, 0, 0, 1); \
             gl.clear(gl.COLOR_BUFFER_BIT); \
             var px = new Uint8Array(4); \
             gl.readPixels(0, 0, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, px); \
             [px[0], px[1], px[2], px[3]].join(',');",
            "webgl_clear",
        )
        .expect("webgl clear/readPixels must not throw");
    assert_eq!(out, "255,0,0,255");
}

#[test]
fn webgl_viewport_and_draw_arrays_rasterise_points() {
    let mut rt = runtime();
    let out = rt
        .eval(
            "var c = document.createElement('canvas'); c.width = 4; c.height = 4; \
             document.body.appendChild(c); \
             var gl = c.getContext('webgl'); \
             gl.viewport(0, 0, 4, 4); \
             gl.clearColor(1, 0, 0, 1); gl.clear(gl.COLOR_BUFFER_BIT); \
             gl.enable(gl.DEPTH_TEST); \
             gl.vertices = new Float32Array([0, 0]); \
             gl.drawArrays(gl.POINTS, 0, 1); \
             var center = new Uint8Array(4); \
             gl.readPixels(2, 2, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, center); \
             var corner = new Uint8Array(4); \
             gl.readPixels(0, 0, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, corner); \
             [center.join(','), corner.join(','), c.getContext('2d') === null].join('|');",
            "webgl_points",
        )
        .expect("webgl drawArrays must not throw");
    assert_eq!(out, "255,255,255,255|255,0,0,255|true");
}

#[test]
fn webgl_read_pixels_honors_array_byte_offset_and_bounds() {
    let mut rt = runtime();
    let out = rt
        .eval(
            "var c = document.createElement('canvas'); c.width = 2; c.height = 2; \
             var gl = c.getContext('webgl'); \
             gl.clearColor(0, 1, 0, 1); gl.clear(gl.COLOR_BUFFER_BIT); \
             var buf = new Uint8Array(8); \
             gl.readPixels(0, 0, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, buf.subarray(2, 6)); \
             var outside = new Uint8Array([9, 9, 9, 9]); \
             gl.readPixels(2, 0, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, outside); \
             [buf[2], buf[3], buf[4], buf[5], outside.join(',')].join('|');",
            "webgl_byte_offset",
        )
        .expect("webgl readPixels must not throw");
    assert_eq!(out, "0|255|0|255|9,9,9,9");
}
