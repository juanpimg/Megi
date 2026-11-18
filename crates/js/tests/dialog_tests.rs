//! DOM/JS integration tests for HTMLDialogElement (`open`, `returnValue`, `show`, `showModal`, `close`).

#![cfg(feature = "js")]

use js::JsRuntime;

#[test]
fn dialog_show_show_modal_close_and_return_value() {
    let doc = html::parse_document(
        "<dialog id='d'><button id='first'>OK</button><input id='af' autofocus></dialog>",
        true,
    );
    let mut runtime =
        JsRuntime::with_document(doc, "https://example.test/".to_owned()).expect("runtime");

    let summary = runtime
        .eval(
            r#"
            const d = document.getElementById("d");
            const initialOpen = d.open;
            const initialRet = d.returnValue;
            d.show();
            const afterShowOpen = d.open;
            const afterShowModalAttr = d.hasAttribute("data-megi-modal");
            d.show(); // no-op when already non-modally open

            let showModalError = "";
            try { d.showModal(); } catch (e) { showModalError = e.name + ":" + (e instanceof DOMException); }

            let closeCount = 0;
            let closeBubbles = null;
            let inlineFired = false;
            d.addEventListener("close", (e) => { closeCount++; closeBubbles = e.bubbles; });
            d.onclose = () => { inlineFired = true; };
            d.close("ok");
            const afterCloseOpen = d.open;
            const afterCloseRet = d.returnValue;

            d.showModal();
            const modalOpen = d.open;
            const hasModalAttr = d.hasAttribute("data-megi-modal");
            const activeId = document.activeElement ? document.activeElement.id : "";

            let showOnModalError = "";
            try { d.show(); } catch (e) { showOnModalError = e.name; }

            d.open = false; // property setter closes without firing close event
            const afterPropOpen = d.open;
            const modalClearedAfterProp = !d.hasAttribute("data-megi-modal");

            const detached = document.createElement("dialog");
            let detachedError = "";
            try { detached.showModal(); } catch (e) { detachedError = e.name + ":" + (e instanceof DOMException); }

            [
              initialOpen,
              initialRet,
              afterShowOpen,
              afterShowModalAttr,
              showModalError,
              closeCount,
              closeBubbles,
              inlineFired,
              afterCloseOpen,
              afterCloseRet,
              modalOpen,
              hasModalAttr,
              activeId,
              showOnModalError,
              afterPropOpen,
              modalClearedAfterProp,
              detachedError
            ].join("|");
            "#,
            "dialog.js",
        )
        .expect("eval");

    assert_eq!(
        summary,
        "false||true|false|InvalidStateError:true|1|false|true|false|ok|true|true|af|InvalidStateError|false|true|InvalidStateError:true"
    );

    let out_doc = runtime.document();
    let d_node = out_doc.get_element_by_id("d").unwrap();
    let el = out_doc.node(d_node).as_element().unwrap();
    assert!(el.get_attribute("open").is_none());
    assert!(el.get_attribute("data-megi-modal").is_none());
}
