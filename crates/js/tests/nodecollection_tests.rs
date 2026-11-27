#![cfg(feature = "js")]

//! WHATWG DOM §4.10/§4.2.3: collection-returning APIs expose a `NodeList`
//! (`item`, `forEach`, read-only `length`), never a plain `Array`.

use megi_js::{Budget, JsRuntime};

fn runtime() -> JsRuntime {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime.set_budget(Budget::SMALL);
    runtime
}

#[test]
fn class_and_selector_queries_expose_node_list_api() {
    let mut runtime = runtime();
    runtime
        .eval(
            "document.write('<p class=\"nl\">a</p><p class=\"nl\">b</p>');
             function shape(list) {
               var first = 'none';
               try { first = list.item(0) ? list.item(0).tagName : 'null'; }
               catch (error) { first = 'THREW'; }
               var seen = '';
               list.forEach(function (node) { seen += node.tagName; });
               var writable = Object.getOwnPropertyDescriptor(list, 'length').writable;
               return [typeof list.item, first, seen, writable].join('|');
             }
             globalThis.byClass = shape(document.getElementsByClassName('nl'));
             globalThis.bySelector = shape(document.querySelectorAll('p.nl'));",
            "nodecollection.js",
        )
        .expect("query APIs");
    assert_eq!(
        runtime
            .eval("byClass + '#' + bySelector", "nodecollection.js")
            .expect("collection shape"),
        "function|P|PP|false#function|P|PP|false"
    );
}
