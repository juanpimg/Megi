use js::JsRuntime;
fn main() {
    let mut r = JsRuntime::new().expect("runtime");
    // Which XHR implementation wins: xhr.rs has `upload`, gap-prelude does not... both define?
    // gap-prelude XHR lacks `upload` and setRequestHeader validation; xhr.rs has both.
    let out = r.eval(
        "var x = new XMLHttpRequest(); \
         'upload=' + (x.upload ? 'yes' : 'no') + \
         '|hasGetAll=' + (typeof x.getAllResponseHeaders) + \
         '|asyncReject=' + (function(){ try { x.open('GET','http://e.com',false); return 'no-throw'; } catch (e){ return 'threw'; } })()",
        "r11.js",
    );
    println!("R11 XHR impl: {out:?}");
}
