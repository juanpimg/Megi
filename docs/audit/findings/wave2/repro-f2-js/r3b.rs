use megi_js::JsRuntime;
use std::time::Instant;
fn main() {
    let mut r = JsRuntime::new().expect("runtime");
    r.eval(
        "var c = document.createElement('canvas'); document.body.appendChild(c); \
         var ctx = c.getContext('2d');",
        "r3.js",
    )
    .expect("setup");
    for size in [1000.0, 2000.0, 4000.0] {
        let t = Instant::now();
        r.eval(&format!("ctx.fillRect(0, 0, {size}, {size});"), "r3.js")
            .expect("fill");
        println!("fillRect({size}x{size}): {:?}", t.elapsed());
    }
    let out = r.eval(
        "var c2 = document.createElement('canvas'); document.body.appendChild(c2); \
         var ctx2 = c2.getContext('2d'); ctx2.fillStyle = '#00ff00'; \
         ctx.__megi_canvas_node = ctx2.__megi_canvas_node; \
         ctx.fillStyle = '#ff0000'; \
         ctx.fillRect(0,0,1,1); \
         'c1=' + c.getContext('2d').fillStyle + '|c2=' + ctx2.fillStyle",
        "r8b.js",
    );
    println!("R8b canvas handle write: {out:?}");
}
