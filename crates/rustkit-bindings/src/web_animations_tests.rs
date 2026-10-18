//! `Element.animate` samples opacity and transform onto inline style, and a
//! bare element id resolves on `window`.

use super::*;

const PAGE: &str = "<html><body><div id=box></div><video id=videoElement></video><svg><path id=dash></path></svg></body></html>";

fn bound() -> DomBindings {
    let b = DomBindings::new(JsRuntime::new().unwrap()).unwrap();
    let doc = Rc::new(Document::parse_html(PAGE).unwrap());
    b.set_document(doc).unwrap();
    b
}

fn ev(b: &DomBindings, script: &str) -> String {
    match b.evaluate(script).unwrap() {
        JsValue::String(s) => s,
        JsValue::Boolean(x) => x.to_string(),
        JsValue::Number(n) => format!("{n}"),
        JsValue::Undefined => "undefined".into(),
        JsValue::Null => "null".into(),
        other => format!("{other:?}"),
    }
}

#[test]
fn animate_samples_opacity_and_finishes_forwards() {
    let b = bound();
    assert_eq!(
        ev(
            &b,
            "var el = document.getElementById('box');
             var anim = el.animate([{opacity:0},{opacity:1}], {duration:1000, fill:'forwards'});
             anim.playState"
        ),
        "running"
    );
    let ran = b.run_timers(496, 100).unwrap();
    assert!(ran > 10, "frames ran: {ran}");
    let opacity: f64 = ev(&b, "getComputedStyle(document.getElementById('box')).opacity")
        .parse()
        .expect("opacity");
    assert!(
        (0.40..=0.60).contains(&opacity),
        "mid-animation opacity {opacity}"
    );
    assert_eq!(ev(&b, "document.getElementById('box').getAnimations().length"), "1");
    assert_eq!(
        ev(
            &b,
            "var anim = document.getElementById('box').getAnimations()[0];
             anim.finish();
             getComputedStyle(document.getElementById('box')).opacity + ' ' + anim.playState"
        ),
        "1 finished"
    );
    assert_eq!(
        ev(
            &b,
            "var seen = 'pending';
             document.getElementById('box').getAnimations()[0].finished.then(function () { seen = 'done'; });
             seen"
        ),
        "pending"
    );
    assert_eq!(ev(&b, "seen"), "done");
    let timeline: f64 = ev(&b, "document.timeline.currentTime").parse().unwrap();
    assert!(timeline >= 496.0, "timeline {timeline}");
}

#[test]
fn cancel_restores_the_inline_style_and_transform_interpolates() {
    let b = bound();
    ev(
        &b,
        "var el = document.getElementById('box');
         el.animate([{transform:'translateX(0px)'},{transform:'translateX(100px)'}], 1000);",
    );
    b.run_timers(496, 100).unwrap();
    let transform = ev(&b, "getComputedStyle(document.getElementById('box')).transform");
    assert!(
        transform.contains("translateX("),
        "transform was not applied: {transform}"
    );
    let px: f64 = transform
        .trim_start_matches("translateX(")
        .trim_end_matches("px)")
        .trim()
        .parse()
        .unwrap_or_else(|_| panic!("transform {transform}"));
    assert!((40.0..=60.0).contains(&px), "translateX {px} from {transform}");
    assert_eq!(
        ev(
            &b,
            "var el = document.getElementById('box');
             el.getAnimations()[0].cancel();
             el.style.transform + ' ' + el.getAnimations().length"
        ),
        " 0"
    );
}

#[test]
fn stroke_dashoffset_and_a_short_duration_finish() {
    let b = bound();
    assert_eq!(
        ev(
            &b,
            "var path = document.getElementById('dash');
             var anim = path.animate([{strokeDashoffset:100},{strokeDashoffset:0}], {duration:0, fill:'forwards'});
             anim.playState"
        ),
        "finished"
    );
    assert_eq!(
        ev(&b, "getComputedStyle(document.getElementById('dash')).strokeDashoffset"),
        "0"
    );
    assert_eq!(
        ev(
            &b,
            "var el = document.getElementById('box');
             var anim = el.animate([{opacity:0},{opacity:1}], 10);
             anim.cancel();
             anim.playState"
        ),
        "idle"
    );
}

#[test]
fn a_named_element_is_a_window_property() {
    let b = bound();
    assert_eq!(
        ev(
            &b,
            "videoElement === document.getElementById('videoElement') && window.videoElement === videoElement"
        ),
        "true"
    );
    assert_eq!(
        ev(&b, "try { notARealElement; 'defined'; } catch (e) { e.name; }"),
        "ReferenceError"
    );
}
