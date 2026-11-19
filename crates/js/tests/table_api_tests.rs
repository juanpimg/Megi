//! #1974: HTMLTableElement / HTMLTableRowElement DOM API.

#![cfg(feature = "js")]

use megi_js::runtime::JsRuntime;

#[test]
fn table_collections_and_mutators() {
    let mut runtime = JsRuntime::new().expect("runtime");
    let out = runtime
        .eval(
            "var table = document.createElement('table'); \
             document.body.appendChild(table); \
             var head = document.createElement('thead'); \
             var hr = document.createElement('tr'); \
             var th = document.createElement('th'); th.textContent = 'H'; \
             hr.appendChild(th); head.appendChild(hr); table.appendChild(head); \
             var body = document.createElement('tbody'); \
             var br = document.createElement('tr'); \
             var td = document.createElement('td'); td.textContent = 'A'; \
             br.appendChild(td); body.appendChild(br); table.appendChild(body); \
             var added = table.insertRow(-1); added.insertCell(-1).textContent = 'B'; \
             var info = table.rows.length + ':' + table.tHead.tagName + ':' + \
                        table.tBodies.length + ':' + added.cells.length + ':' + \
                        table.rows.item(2).cells.item(0).textContent; \
             table.deleteRow(1); \
             info += ':' + table.rows.length; \
             info;",
            "table.js",
        )
        .expect("eval");
    assert_eq!(out, "3:THEAD:1:1:B:2");
}

#[test]
fn create_t_body_appends_a_section() {
    let mut runtime = JsRuntime::new().expect("runtime");
    let out = runtime
        .eval(
            "var table = document.createElement('table'); \
             var b = table.createTBody(); b.insertRow(0).insertCell(0).textContent = 'X'; \
             String(b.tagName) + ':' + table.tBodies.length + ':' + table.rows.length + ':' + \
             table.rows.item(0).cells.item(0).textContent;",
            "table2.js",
        )
        .expect("eval");
    assert_eq!(out, "TBODY:1:1:X");
}
