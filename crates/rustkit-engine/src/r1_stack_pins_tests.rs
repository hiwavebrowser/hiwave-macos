//! Pins from the independent review of the Speedometer stack (R1 MED 1, 2
//! and 5).

use super::*;

fn page(html: &str) -> (Engine, EngineViewId) {
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 800, height: 600 })
        .expect("view");
    engine.load_html(view, html).expect("load");
    (engine, view)
}

fn js(engine: &mut Engine, view: EngineViewId, script: &str) -> String {
    let value = engine.execute_script(view, script).expect("script");
    value.strip_prefix("String(\"").and_then(|v| v.strip_suffix("\")")).unwrap_or(&value).to_string()
}

/// MED 5: the engine's own entry points are not the page's to call or to
/// replace, and nothing the page does makes an event trusted.
#[test]
fn a_page_cannot_reach_the_engines_event_internals_or_forge_trust() {
    let (mut engine, view) = page(
        "<!doctype html><body style=\"margin:0\"><input id=f style=\"display:block\">\
         <svg id=s width=200 height=100 style=\"display:block\"><rect id=r width=200 height=100></rect></svg>\
         <script>var seen = [];\
         document.addEventListener('keydown', function (e) { seen.push('keydown ' + e.isTrusted); }, true);\
         document.addEventListener('textInput', function (e) { seen.push('textInput ' + e.isTrusted + ' ' + e.data); }, true);\
         document.addEventListener('mousemove', function (e) { seen.push('mousemove ' + e.target.id + ' ' + e.isTrusted); }, true);\
         document.addEventListener('x', function (e) { seen.push('x ' + e.isTrusted); }, true);</script>",
    );
    // None of the stack's internals is a property the page can read.
    assert_eq!(
        js(
            &mut engine,
            view,
            "[typeof document.__rkFireTextInput, typeof document.__rkFireKey, typeof window.__rkNewTextEvent, \
              typeof window.__rkSvgHit, typeof document.__rkViewFocused].join(' ')"
        ),
        "undefined undefined undefined undefined undefined"
    );
    // Defining them changes nothing the engine does.
    js(
        &mut engine,
        view,
        "window.__rkSvgHit = function () { return document.body; }; \
         document.__rkFireTextInput = function () { seen.push('page ran the text hook'); return false; }; \
         document.__rkFireKey = function () { seen.push('page ran the key hook'); return false; }; \
         window.__rkNewTextEvent = function () { seen.push('page made the text event'); }; \
         document.__rkViewFocused = false; 0",
    );
    assert_eq!(js(&mut engine, view, "String(document.hasFocus())"), "true", "the page cannot unfocus itself");
    engine.mouse_move_at_point(view, 50.0, 60.0);
    assert_eq!(engine.focus_at_point(view, 20.0, 8.0).as_deref(), Some("input"));
    engine.handle_text_key(view, 0, "q", false, false, false);
    // What script dispatches is never trusted, whatever it writes first.
    js(
        &mut engine,
        view,
        "var e = new Event('x'); try { e.isTrusted = true; } catch (_) {} document.dispatchEvent(e); \
         var k = new KeyboardEvent('keydown', { key: 'z' }); try { k.isTrusted = true; } catch (_) {} document.dispatchEvent(k); \
         try { Object.defineProperty(k, 'isTrusted', { value: true }); } catch (_) {} seen.push('redefined ' + k.isTrusted); \
         document.dispatchEvent({ type: 'x', isTrusted: true, bubbles: false, cancelable: false, target: null }); 0",
    );
    assert_eq!(
        js(&mut engine, view, "seen.join(' | ')"),
        "mousemove r true | keydown true | textInput true q | x false | keydown false | redefined false | x false"
    );
}

/// MED 1: a timing, run by hand in the parity profile, before and after:
/// `cargo test -p rustkit-engine --lib --features headless --profile parity r1_svg_pointer_cost -- --ignored --nocapture`
///
/// A chart of 5,000 paths; the pointer moves 200 times over its background
/// (no shape is under it, so every shape is a candidate each time).
#[test]
#[ignore = "a timing, run by hand in the parity profile"]
fn r1_svg_pointer_cost() {
    let mut html = String::from("<!doctype html><body style=\"margin:0\"><svg id=s width=800 height=600 style=\"display:block\">");
    for i in 0..5000 {
        let (x, y) = (10 + (i % 100) * 7, 10 + (i / 100) * 9);
        html.push_str(&format!("<path d=\"M{x} {y} l 3 0 l 0 3 z\"></path>"));
    }
    html.push_str("</svg><script>var n = 0; document.addEventListener('mousemove', function (e) { n++; });</script>");
    let (mut engine, view) = page(&html);
    let started = std::time::Instant::now();
    for i in 0..200 {
        engine.mouse_move_at_point(view, 720.0 + (i % 10) as f32, 500.0 + (i % 7) as f32);
    }
    let moves = started.elapsed().as_secs_f64() * 1000.0;
    let started = std::time::Instant::now();
    let hits = js(&mut engine, view, "var h = 0; for (var i = 0; i < 200; i++) { if (document.elementFromPoint(720 + i % 10, 500 + i % 7).id === 's') h++; } String(h)");
    let reads = started.elapsed().as_secs_f64() * 1000.0;
    println!("5,000 paths: 200 pointer moves {moves:.1} ms ({:.3} ms each); 200 elementFromPoint {reads:.1} ms ({hits} on the svg)", moves / 200.0);
}

/// MED 2: a timing, run by hand in the parity profile, before and after:
/// `cargo test -p rustkit-engine --lib --features headless --profile parity r1_range_rect_cost -- --ignored --nocapture`
///
/// 5,000 paragraphs, one text node each; a range over all of them asks for
/// its bounding rect (what a selection toolbar does on `selectionchange`).
#[test]
#[ignore = "a timing, run by hand in the parity profile"]
fn r1_range_rect_cost() {
    let mut html = String::from("<!doctype html><body style=\"margin:0\"><div id=all>");
    for i in 0..5000 {
        html.push_str(&format!("<p style=\"margin:0\">paragraph {i}</p>"));
    }
    html.push_str("</div>");
    let (mut engine, view) = page(&html);
    js(&mut engine, view, "window.r = document.createRange(); r.selectNodeContents(document.getElementById('all')); 0");
    let started = std::time::Instant::now();
    let rect = js(&mut engine, view, "var b = r.getBoundingClientRect(); [b.width > 0, Math.round(b.height)].join(' ')");
    let bounding = started.elapsed().as_secs_f64() * 1000.0;
    let started = std::time::Instant::now();
    let count = js(&mut engine, view, "String(r.getClientRects().length)");
    let rects = started.elapsed().as_secs_f64() * 1000.0;
    println!("5,000 text nodes: getBoundingClientRect {bounding:.1} ms ({rect}); getClientRects {rects:.1} ms ({count} rects)");
}
