//! Canvas 2D path geometry, transforms, state stack, linear gradients, and drawImage tests.
//!
//! Plan: docs/superpowers/plans/2026-11-24-engine-depth-plan.md §Task 6.
//! Spec: WHATWG HTML §4.12.5.1 (2D Rendering Context).

#![cfg(feature = "js")]

use megi_js::runtime::JsRuntime;

fn runtime() -> JsRuntime {
    JsRuntime::new().expect("runtime")
}

#[test]
fn arc_respects_start_and_end_angle() {
    let mut rt = runtime();
    let out = rt
        .eval(
            "var c = document.createElement('canvas'); c.width = 100; c.height = 100; \
             var ctx = c.getContext('2d'); \
             ctx.fillStyle = '#ff0000'; \
             ctx.beginPath(); \
             ctx.arc(50, 50, 20, 0, Math.PI); \
             ctx.fill(); \
             var lower = ctx.getImageData(50, 60, 1, 1).data; \
             var upper = ctx.getImageData(50, 40, 1, 1).data; \
             [lower[0], lower[1], lower[2], lower[3], upper[0], upper[1], upper[2], upper[3]].join(',');",
            "arc_angles",
        )
        .expect("arc test must evaluate");
    assert_eq!(out, "255,0,0,255,0,0,0,0");
}

#[test]
fn bezier_draws_expected_pixels() {
    let mut rt = runtime();
    let out = rt
        .eval(
            "var c = document.createElement('canvas'); c.width = 100; c.height = 60; \
             var ctx = c.getContext('2d'); \
             ctx.strokeStyle = '#ff0000'; \
             ctx.lineWidth = 3; \
             ctx.beginPath(); \
             ctx.moveTo(10, 50); \
             ctx.bezierCurveTo(30, 10, 70, 10, 90, 50); \
             ctx.stroke(); \
             var apex = ctx.getImageData(50, 20, 1, 1).data; \
             var below = ctx.getImageData(50, 45, 1, 1).data; \
             var c2 = document.createElement('canvas'); c2.width = 40; c2.height = 40; \
             var q = c2.getContext('2d'); \
             q.fillStyle = '#00ff00'; \
             q.beginPath(); q.moveTo(0, 30); q.quadraticCurveTo(20, 0, 40, 30); q.closePath(); q.fill(); \
             var qmid = q.getImageData(20, 22, 1, 1).data; \
             [apex[0], apex[3], below[3], qmid[1], qmid[3]].join(',');",
            "bezier",
        )
        .expect("bezier test must evaluate");
    assert_eq!(out, "255,255,0,255,255");
}

#[test]
fn drawimage_scales_with_globalalpha() {
    let mut rt = runtime();
    let out = rt
        .eval(
            "var src = document.createElement('canvas'); src.width = 2; src.height = 2; \
             var sctx = src.getContext('2d'); \
             sctx.fillStyle = '#ff0000'; sctx.fillRect(0, 0, 2, 2); \
             var dst = document.createElement('canvas'); dst.width = 10; dst.height = 10; \
             var dctx = dst.getContext('2d'); \
             dctx.globalAlpha = 0.5; \
             dctx.drawImage(src, 2, 2, 4, 4); \
             var hit = dctx.getImageData(3, 3, 1, 1).data; \
             var tl = dctx.getImageData(0, 0, 1, 1).data; \
             var br = dctx.getImageData(7, 7, 1, 1).data; \
             [hit[0], hit[1], hit[2], hit[3], tl[3], br[3]].join(',');",
            "drawimage",
        )
        .expect("drawImage test must evaluate");
    let parts: Vec<u8> = out
        .split(',')
        .map(|s| s.parse::<u8>().expect("u8"))
        .collect();
    assert_eq!(parts[0], 255);
    assert_eq!(parts[1], 0);
    assert_eq!(parts[2], 0);
    assert!(
        (126..=129).contains(&parts[3]),
        "alpha should be ~128, got {}",
        parts[3]
    );
    assert_eq!(parts[4], 0);
    assert_eq!(parts[5], 0);
}

#[test]
fn transform_save_restore_and_rect_path() {
    let mut rt = runtime();
    let out = rt
        .eval(
            "var c = document.createElement('canvas'); c.width = 30; c.height = 30; \
             var ctx = c.getContext('2d'); \
             ctx.fillStyle = '#0000ff'; \
             ctx.save(); \
             ctx.translate(10, 10); \
             ctx.scale(2, 2); \
             ctx.fillRect(0, 0, 5, 5); \
             ctx.restore(); \
             ctx.fillStyle = '#00ff00'; \
             ctx.beginPath(); ctx.rect(0, 0, 4, 4); ctx.fill(); \
             var p1 = ctx.getImageData(15, 15, 1, 1).data; \
             var p2 = ctx.getImageData(2, 2, 1, 1).data; \
             var p3 = ctx.getImageData(22, 22, 1, 1).data; \
             [p1[2], p1[3], p2[1], p2[3], p3[3]].join(',');",
            "transform_save_restore",
        )
        .expect("transform test must evaluate");
    assert_eq!(out, "255,255,255,255,0");
}

#[test]
fn linear_gradient_fill_rect_interpolates_stops() {
    let mut rt = runtime();
    let out = rt
        .eval(
            "var c = document.createElement('canvas'); c.width = 20; c.height = 4; \
             var ctx = c.getContext('2d'); \
             var g = ctx.createLinearGradient(0, 0, 20, 0); \
             g.addColorStop(0, '#ff0000'); \
             g.addColorStop(1, '#0000ff'); \
             ctx.fillStyle = g; \
             ctx.fillRect(0, 0, 20, 4); \
             var l = ctx.getImageData(0, 1, 1, 1).data; \
             var m = ctx.getImageData(10, 1, 1, 1).data; \
             var r = ctx.getImageData(19, 1, 1, 1).data; \
             [l[0], l[2], m[0], m[2], r[0], r[2]].join(',');",
            "gradient",
        )
        .expect("gradient test must evaluate");
    let parts: Vec<u8> = out
        .split(',')
        .map(|s| s.parse::<u8>().expect("u8"))
        .collect();
    assert!(parts[0] > 230 && parts[1] < 25, "left red: {parts:?}");
    assert!(
        (100..=155).contains(&parts[2]) && (100..=155).contains(&parts[3]),
        "mid blend: {parts:?}"
    );
    assert!(parts[5] > 230 && parts[4] < 25, "right blue: {parts:?}");
}

#[test]
fn radial_gradient_fill_rect_interpolates_from_center() {
    // WHATWG HTML §4.12.5.1.5: createRadialGradient paints stops along the cone
    // between the two circles, so the inner stop colors the center and the outer
    // stop colors the rim (#M6 Canvas 2D).
    let mut rt = runtime();
    let out = rt
        .eval(
            "var c = document.createElement('canvas'); c.width = 40; c.height = 40; \
             var ctx = c.getContext('2d'); \
             var g = ctx.createRadialGradient(20, 20, 0, 20, 20, 20); \
             g.addColorStop(0, '#ff0000'); g.addColorStop(1, '#0000ff'); \
             ctx.fillStyle = g; ctx.fillRect(0, 0, 40, 40); \
             var center = ctx.getImageData(20, 20, 1, 1).data; \
             var rim = ctx.getImageData(39, 20, 1, 1).data; \
             [center[0], center[2], rim[0], rim[2]].join(',');",
            "radial_gradient",
        )
        .expect("createRadialGradient must be available");
    let parts: Vec<u8> = out
        .split(',')
        .map(|s| s.parse::<u8>().expect("u8"))
        .collect();
    assert!(parts[0] > 200 && parts[1] < 55, "center red: {parts:?}");
    assert!(parts[2] < 55 && parts[3] > 200, "rim blue: {parts:?}");
}
