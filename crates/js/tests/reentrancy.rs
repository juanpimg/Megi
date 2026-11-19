//! Re-entrancy: nested synchronous event dispatch must not double-borrow the host
//! (Task 4.1 RED fixture).

#![cfg(feature = "js")]

use megi_js::JsRuntime;

#[test]
fn nested_event_dispatch_can_mutate_dom_reentrantly() {
    let mut rt = JsRuntime::new().expect("runtime");
    rt.eval(
        "window.__log = [];
         var a = document.createElement('div');
         var b = document.createElement('div');
         document.body.appendChild(a);
         document.body.appendChild(b);
         a.addEventListener('click', function () {
             __log.push('outer');
             b.dispatchEvent(new Event('click'));
             a.setAttribute('data-after', '1');
         });
         b.addEventListener('click', function () {
             __log.push('inner');
             b.setAttribute('data-inner', '1');
         });
         a.dispatchEvent(new Event('click'));",
        "reentrancy.js",
    )
    .expect("setup");
    assert_eq!(
        rt.eval("__log.join(',')", "reentrancy.js").expect("log"),
        "outer,inner"
    );
}
