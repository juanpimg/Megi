//! CSS animation/transition DOM event regression test (#941, #942).

use megi_html::parse_document;
use megi_js::{JsEventTarget, JsRuntime, JsUiEventKind};

#[test]
fn animation_and_transition_events_expose_webidl_fields_and_bubble() {
    let doc = parse_document(
        "<!doctype html><html><body><div id='box'></div></body></html>",
        false,
    );
    let mut runtime =
        JsRuntime::with_document(doc, "https://example.test/".to_owned()).expect("js runtime");
    runtime
        .eval(
            r#"
            const log = [];
            const box = document.getElementById('box');
            const customAnim = new AnimationEvent('animationend', {
                bubbles: true,
                animationName: 'slide',
                elapsedTime: 0.75,
                pseudoElement: '::before'
            });
            const customTrans = new TransitionEvent('transitionend', {
                bubbles: true,
                propertyName: 'width',
                elapsedTime: 0.5
            });
            if (!(customAnim instanceof AnimationEvent) || !(customTrans instanceof TransitionEvent) || typeof customAnim.preventDefault !== 'function') {
                throw new Error('must be instances of AnimationEvent / TransitionEvent with Event methods');
            }
            box.onanimationstart = (e) => log.push('inline-animstart:' + e.animationName + ':' + e.elapsedTime);
            box.ontransitionend = (e) => log.push('inline-transend:' + e.propertyName + ':' + e.elapsedTime);
            document.addEventListener('animationend', (e) => {
                log.push('bubble-animend:' + e.animationName + ':' + e.elapsedTime + ':' + e.pseudoElement);
            });
            box.addEventListener('transitionrun', (e) => {
                log.push('transrun:' + e.propertyName + ':' + e.elapsedTime);
            });
            box.dispatchEvent(customAnim);
            box.dispatchEvent(customTrans);
            "#,
            "anim_trans_events.js",
        )
        .expect("eval anim_trans_events");

    let box_id = {
        let d = runtime.document();
        d.get_elements_by_tag_name("div")[0]
    };

    runtime
        .dispatch_ui_event(
            JsEventTarget::Node(box_id),
            "animationstart",
            true,
            false,
            &JsUiEventKind::Animation {
                animation_name: "fade".to_owned(),
                elapsed_time: 0.0,
                pseudo_element: String::new(),
            },
        )
        .expect("animationstart");
    runtime
        .dispatch_ui_event(
            JsEventTarget::Node(box_id),
            "animationend",
            true,
            false,
            &JsUiEventKind::Animation {
                animation_name: "fade".to_owned(),
                elapsed_time: 1.0,
                pseudo_element: String::new(),
            },
        )
        .expect("animationend");
    runtime
        .dispatch_ui_event(
            JsEventTarget::Node(box_id),
            "transitionrun",
            true,
            false,
            &JsUiEventKind::Transition {
                property_name: "opacity".to_owned(),
                elapsed_time: 0.0,
                pseudo_element: String::new(),
            },
        )
        .expect("transitionrun");
    runtime
        .dispatch_ui_event(
            JsEventTarget::Node(box_id),
            "transitionend",
            true,
            false,
            &JsUiEventKind::Transition {
                property_name: "opacity".to_owned(),
                elapsed_time: 0.5,
                pseudo_element: String::new(),
            },
        )
        .expect("transitionend");

    let actual = runtime
        .eval("log.join('|')", "read_log.js")
        .expect("read log");
    assert_eq!(
        actual,
        "bubble-animend:slide:0.75:::before|inline-transend:width:0.5|inline-animstart:fade:0|bubble-animend:fade:1:|transrun:opacity:0|inline-transend:opacity:0.5"
    );
}
