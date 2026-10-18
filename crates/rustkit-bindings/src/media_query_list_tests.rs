//! `matchMedia` (CSSOM View) against the cascade's media evaluator.

use super::*;

fn fresh() -> DomBindings {
    DomBindings::new(JsRuntime::new().unwrap()).unwrap()
}

fn ev(bindings: &DomBindings, script: &str) -> String {
    match bindings.evaluate(script).unwrap() {
        JsValue::String(s) => s,
        JsValue::Boolean(b) => b.to_string(),
        JsValue::Number(n) => {
            if n.fract() == 0.0 {
                format!("{}", n as i64)
            } else {
                n.to_string()
            }
        }
        JsValue::Null => "null".to_string(),
        JsValue::Undefined => "undefined".to_string(),
        other => panic!("{script} evaluated to {other:?}"),
    }
}

const PROBE: &str = r#"
(function () {
  var changes = [];
  var q = matchMedia('(min-width: 900px)');
  q.addEventListener('change', function (e) {
    changes.push('add:' + e.matches + ':' + e.media + ':' + (e instanceof MediaQueryListEvent) + ':' + (e.target === q));
  });
  q.addListener(function (e) { changes.push('legacy:' + e.matches); });
  q.onchange = function (e) { changes.push('on:' + e.matches); };
  window.__q = q;
  window.__changes = changes;
  return [q.matches, q.media, q instanceof MediaQueryList, q instanceof EventTarget, typeof q.addListener, typeof q.removeListener, devicePixelRatio, visualViewport.width, visualViewport.scale, visualViewport.offsetLeft].join('|');
})()
"#;

#[test]
fn a_query_matches_the_viewport_and_flips_when_it_changes() {
    let b = fresh();
    // The bindings' placeholder viewport is 800x600, device pixel ratio 1.
    assert_eq!(
        ev(&b, PROBE),
        "false|(min-width: 900px)|true|true|function|function|1|800|1|0"
    );
    assert_eq!(ev(&b, "matchMedia('(max-width: 900px)').matches"), "true");
    assert_eq!(
        ev(&b, "matchMedia('print, (min-width: 100px)').matches"),
        "true"
    );
    assert_eq!(ev(&b, "matchMedia('not screen').matches"), "false");
    assert_eq!(ev(&b, "matchMedia('').matches"), "true");
    assert_eq!(ev(&b, "matchMedia('(frobnicate: yes)').matches"), "false");
    assert_eq!(
        ev(&b, "matchMedia('(min-resolution: 2dppx)').matches"),
        "false"
    );
    assert_eq!(
        ev(&b, "matchMedia('(min-width: 48em)').matches"),
        "true",
        "48em is 768px, and the placeholder viewport is 800 wide"
    );

    b.set_dimensions(1280.0, 696.0).unwrap();
    assert_eq!(ev(&b, "window.__q.matches"), "true");
    assert_eq!(
        ev(&b, "visualViewport.width + 'x' + visualViewport.height"),
        "1280x696"
    );
    assert_eq!(
        ev(&b, "window.__changes.join(',')"),
        "add:true:(min-width: 900px):true:true,legacy:true,on:true"
    );
    let vv = ev(&b, "var n = 0; visualViewport.addEventListener('resize', function () { n++; }); window.__vv = function () { return n; }; '0'");
    assert_eq!(vv, "0");

    // Still matching: no second change, and the visual viewport did resize
    // with the previous set_dimensions (before this listener existed).
    b.set_dimensions(1000.0, 696.0).unwrap();
    assert_eq!(ev(&b, "window.__q.matches"), "true");
    assert_eq!(ev(&b, "window.__changes.length"), "3");
    assert_eq!(ev(&b, "String(window.__vv())"), "1");

    b.set_dimensions(800.0, 600.0).unwrap();
    assert_eq!(ev(&b, "window.__q.matches"), "false");
    assert_eq!(ev(&b, "window.__changes.length"), "6");
    assert_eq!(ev(&b, "String(window.__vv())"), "2");
}

#[test]
fn listeners_can_be_removed_and_a_missing_query_throws() {
    let b = fresh();
    assert_eq!(
        ev(
            &b,
            r#"
            var n = 0;
            var q = matchMedia('(min-width: 1000px)');
            function on() { n++; }
            function legacy() { n++; }
            q.addEventListener('change', on);
            q.addListener(legacy);
            window.__n = function () { return n; };
            window.__q2 = q;
            'ok'
            "#
        ),
        "ok"
    );
    b.set_dimensions(1100.0, 600.0).unwrap();
    assert_eq!(ev(&b, "String(window.__n())"), "2");
    ev(
        &b,
        "window.__q2.removeEventListener('change', on); window.__q2.removeListener(legacy);",
    );
    b.set_dimensions(800.0, 600.0).unwrap();
    assert_eq!(ev(&b, "String(window.__n())"), "2");
    assert_eq!(
        ev(
            &b,
            "var name; try { matchMedia(); name = 'no throw'; } catch (e) { name = e.name; } name"
        ),
        "TypeError"
    );
    assert_eq!(
        ev(&b, "var name; try { new MediaQueryList(); name = 'no throw'; } catch (e) { name = e.name; } name"),
        "TypeError"
    );
    assert_eq!(
        ev(&b, "var e = new MediaQueryListEvent('change', { media: '(min-width: 1px)', matches: true }); [e.media, e.matches, e instanceof Event].join('|')"),
        "(min-width: 1px)|true|true"
    );
}

#[test]
fn device_pixel_ratio_flips_resolution_queries_without_a_resize() {
    let b = fresh();
    let q = ev(
        &b,
        r#"
        var n = 0, vv = 0;
        var q = matchMedia('(min-resolution: 2dppx), (-webkit-min-device-pixel-ratio: 2)');
        q.addEventListener('change', function (e) { n++; window.__last = e.matches; });
        visualViewport.addEventListener('resize', function () { vv++; });
        window.__n = function () { return n; };
        window.__vv = function () { return vv; };
        String(q.matches)
        "#,
    );
    assert_eq!(q, "false");
    b.set_device_pixel_ratio(2.0).unwrap();
    assert_eq!(ev(&b, "String(devicePixelRatio)"), "2");
    assert_eq!(
        ev(&b, "String(matchMedia('(min-resolution: 192dpi)').matches)"),
        "true"
    );
    assert_eq!(
        ev(&b, "String(matchMedia('(max-resolution: 1dppx)').matches)"),
        "false"
    );
    assert_eq!(ev(&b, "String(window.__n())"), "1");
    assert_eq!(ev(&b, "String(window.__last)"), "true");
    // Pinch scale stays 1, and a scale change is not a visual-viewport resize.
    assert_eq!(ev(&b, "String(visualViewport.scale)"), "1");
    assert_eq!(ev(&b, "String(window.__vv())"), "0");
    b.set_device_pixel_ratio(2.0).unwrap();
    assert_eq!(
        ev(&b, "String(window.__n())"),
        "1",
        "the same ratio is not a change"
    );
}
