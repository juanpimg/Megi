use megi_js::{Budget, JsRuntime};
use std::time::Instant;

fn rt() -> JsRuntime {
    JsRuntime::new().expect("runtime")
}

fn main() {
    let budget = Budget::default();

    // R1: import('x') inside a string literal must not be treated as a dynamic import.
    {
        let mut r = rt();
        r.set_module_resolver(Box::new(|spec, _base| Ok(Some(format!("r:{spec}")))));
        r.set_module_loader(Box::new(|url| {
            Ok(if url == "r:entry" {
                "var s = \"call import('ghost') now\"; export const v = s;".to_string()
            } else {
                "export const z = 1;".to_string()
            })
        }));
        let out = r.eval_module(
            "var s = \"call import('ghost') now\"; export const v = s;",
            "r1.js",
            "r:entry",
            &budget,
        );
        println!("R1 string-import rewrite: {out:?}");
    }

    // R2: a function call named myimport('x') must not be rewritten.
    {
        let mut r = rt();
        r.set_module_resolver(Box::new(|spec, _base| Ok(Some(format!("r:{spec}")))));
        r.set_module_loader(Box::new(|url| {
            Ok(if url == "r:entry" {
                "function myimport(x) { return x; } export const v = myimport('lit');".to_string()
            } else {
                "export const z = 1;".to_string()
            })
        }));
        let out = r.eval_module(
            "function myimport(x) { return x; } export const v = myimport('lit');",
            "r2.js",
            "r:entry",
            &budget,
        );
        println!("R2 myimport call: {out:?}");
    }

    // R3: canvas fillRect cost must be proportional to the surface, not the rect.
    {
        let mut r = rt();
        r.eval(
            "var c = document.createElement('canvas'); document.body.appendChild(c); \
             var ctx = c.getContext('2d');",
            "r3.js",
        )
        .expect("setup");
        let t = Instant::now();
        r.eval("ctx.fillRect(0, 0, 5000, 5000);", "r3.js")
            .expect("fill");
        println!("R3 fillRect(5000x5000) on 300x150 canvas: {:?}", t.elapsed());
    }

    // R4: writeln must append a newline (HTML spec), not behave like write.
    {
        let mut r = rt();
        r.eval("document.writeln('a'); document.write('b');", "r4.js")
            .expect("write");
        let doc = r.take_document();
        let body = doc.body().expect("body");
        println!("R4 writeln body text: {:?}", doc.text_content(body));
    }

    // R5: CustomEvent cancelable defaults to false (DOM spec).
    {
        let mut r = rt();
        let v = r
            .eval("new CustomEvent('x').cancelable", "r5.js")
            .expect("ce");
        println!("R5 CustomEvent cancelable default: {v}");
    }

    // R6: new Event() with no type must throw TypeError (DOM spec).
    {
        let mut r = rt();
        let out = r.eval("try { new Event(); 'no-throw:' + (new Event().type); } catch (e) { 'threw:' + e.name }", "r6.js");
        println!("R6 Event() no-arg: {out:?}");
    }

    // R7: forged __megi_worker id must be a no-op, never a panic (#1465).
    {
        let mut r = rt();
        r.set_worker_loader(Box::new(|url| {
            if url.ends_with("w.js") {
                Ok("1;".to_string())
            } else {
                Err(megi_js::JsError::Runtime("nf".into()))
            }
        }));
        let out = r.eval(
            "var w = new Worker('w.js'); w.__megi_worker = 1000; \
             try { w.terminate(); 'no-panic'; } catch (e) { 'threw:' + e; }",
            "r7.js",
        );
        println!("R7 forged worker terminate: {out:?}");
    }

    // R8: style object __megi_node must not be script-writable (cf. #644).
    {
        let mut r = rt();
        let out = r.eval(
            "var a = document.createElement('div'); var b = document.createElement('div'); \
             document.body.appendChild(a); document.body.appendChild(b); \
             a.style.__megi_node = b.style.__megi_node; \
             a.style.color = 'red'; \
             'a=' + a.getAttribute('style') + '|b=' + b.getAttribute('style')",
            "r8.js",
        );
        println!("R8 style handle write: {out:?}");
    }

    // R9: setTimeout delay coercion: string "100" per WebIDL ToNumber.
    {
        let mut r = rt();
        r.eval("window.__fired = false; setTimeout(function(){ __fired = true; }, '100');", "r9.js")
            .expect("timer");
        r.run_until(50).expect("pump");
        let v = r.eval("window.__fired", "r9.js").expect("check");
        println!("R9 string-delay fired at t=50 (spec: false): {v}");
    }

    // R10: listener cap probe (MAX_EVENT_LISTENERS = 16384).
    {
        let mut r = rt();
        let out = r.eval(
            "var n = 0; try { for (var i = 0; i < 20000; i++) { addEventListener('t' + i, function(){}); } n = 1; } catch (e) { n = 2; } n;",
            "r10.js",
        );
        println!("R10 listener cap probe: {out:?}");
    }
}
