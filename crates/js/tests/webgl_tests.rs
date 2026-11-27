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

#[test]
fn webgl_triangle_rasterises_a_real_surface_region() {
    let mut rt = runtime();
    let out = rt
        .eval(
            "var c = document.createElement('canvas'); c.width = 8; c.height = 8; \
             document.body.appendChild(c); \
             var gl = c.getContext('webgl'); \
             gl.viewport(0, 0, 8, 8); \
             gl.clearColor(1, 0, 0, 1); gl.clear(gl.COLOR_BUFFER_BIT); \
             gl.vertices = [-0.5, -0.5, 0.5, -0.5, 0, 0.5]; \
             gl.drawArrays(gl.TRIANGLES, 0, 3); \
             var inside = new Uint8Array(4); \
             gl.readPixels(4, 4, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, inside); \
             var outside = new Uint8Array(4); \
             gl.readPixels(0, 0, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, outside); \
             [inside.join(','), outside.join(',')].join('|');",
            "webgl_triangle",
        )
        .expect("webgl triangle drawArrays must not throw");
    assert_eq!(out, "255,255,255,255|255,0,0,255");
}

#[test]
fn webgl_vertex_attrib_pointer_draws_points_from_buffer() {
    let mut rt = runtime();
    let out = rt
        .eval(
            "var c = document.createElement('canvas'); c.width = 4; c.height = 4; \
             document.body.appendChild(c); \
             var gl = c.getContext('webgl'); \
             gl.viewport(0, 0, 4, 4); \
             gl.clearColor(1, 0, 0, 1); gl.clear(gl.COLOR_BUFFER_BIT); \
             var buffer = gl.createBuffer(); \
             gl.bindBuffer(gl.ARRAY_BUFFER, buffer); \
             gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-0.5, -0.5, 0.5, 0.5]), gl.STATIC_DRAW); \
             gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0); \
             gl.drawArrays(gl.POINTS, 0, 2); \
             var first = new Uint8Array(4); \
             gl.readPixels(1, 1, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, first); \
             var second = new Uint8Array(4); \
             gl.readPixels(3, 3, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, second); \
             var middle = new Uint8Array(4); \
             gl.readPixels(2, 2, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, middle); \
             gl.deleteBuffer(buffer); \
             [first.join(','), second.join(','), middle.join(','), \
              gl.getError() === WebGLRenderingContext.NO_ERROR].join('|');",
            "webgl_attrib_buffer",
        )
        .expect("webgl vertexAttribPointer drawArrays must not throw");
    assert_eq!(out, "255,255,255,255|255,255,255,255|255,0,0,255|true");
}

#[test]
fn webgl_buffer_sub_data_accepts_nan_offset_without_error() {
    let mut rt = runtime();
    let out = rt
        .eval(
            "var c = document.createElement('canvas'); \
             var gl = c.getContext('experimental-webgl'); \
             var buffer = gl.createBuffer(); \
             gl.bindBuffer(gl.ARRAY_BUFFER, buffer); \
             gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([1]), gl.STATIC_DRAW); \
             gl.bufferSubData(gl.ARRAY_BUFFER, NaN, new Float32Array([2])); \
             [gl.getError(), WebGLRenderingContext.NO_ERROR, !!buffer].join('|');",
            "webgl_buffer_sub_data",
        )
        .expect("webgl bufferSubData must not throw");
    assert_eq!(out, "0|0|true");
}

#[test]
fn webgl_buffer_data_accepts_dynamic_and_stream_draw() {
    // WebGL 1.0 §5.14.4: bufferData accepts STATIC_DRAW, DYNAMIC_DRAW and
    // STREAM_DRAW. Rejecting the latter two with INVALID_ENUM drops geometry
    // in real pages that upload data with non-static usage hints.
    let mut rt = runtime();
    let out = rt
        .eval(
            "var c = document.createElement('canvas'); c.width = 4; c.height = 4; \
             document.body.appendChild(c); \
             var gl = c.getContext('webgl'); \
             var exposed = [typeof gl.DYNAMIC_DRAW, typeof gl.STREAM_DRAW].join(','); \
             function upload(usage) { \
                 gl.viewport(0, 0, 4, 4); \
                 gl.clearColor(1, 0, 0, 1); gl.clear(gl.COLOR_BUFFER_BIT); \
                 var buffer = gl.createBuffer(); \
                 gl.bindBuffer(gl.ARRAY_BUFFER, buffer); \
                 gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-0.5, -0.5, 0.5, 0.5]), usage); \
                 var error = gl.getError(); \
                 gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0); \
                 gl.drawArrays(gl.POINTS, 0, 2); \
                 var px = new Uint8Array(4); \
                 gl.readPixels(3, 3, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, px); \
                 gl.deleteBuffer(buffer); \
                 return error + ':' + px.join(','); \
             } \
             var dynamic = upload(gl.DYNAMIC_DRAW); \
             var stream = upload(gl.STREAM_DRAW); \
             [exposed, dynamic, stream, gl.getError() === WebGLRenderingContext.NO_ERROR].join('|');",
            "webgl_usage_flags",
        )
        .expect("webgl bufferData usage hints must not throw");
    assert_eq!(
        out,
        "number,number|0:255,255,255,255|0:255,255,255,255|true"
    );
}
