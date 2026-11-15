//! STYLE1b (a): what each stage of an inline style write costs today, for the plan's (b) to (e).
//! Measurement only (`--ignored`); run it in an optimised build:
//!
//! `cargo test --profile parity -p rustkit-bindings --lib style1b_cost -- --ignored --nocapture`
//!
//! The page is MotionMark Multiply's shape at the size #1170 measured: 5000 tiles, each with three
//! inline properties, all written once per frame (15000 writes). Each stage runs in one script turn
//! and prints microseconds per operation; the empty host call is the floor any native write sits on.

use super::*;

const TILES: usize = 5000;

fn page() -> DomBindings {
    let mut runtime = JsRuntime::new().unwrap();
    // The floor: a host function that does nothing.
    runtime.register_host_function("__style1b_noop", 0, Box::new(|_| JsValue::Undefined)).unwrap();
    let b = DomBindings::new(runtime).unwrap();
    let mut html = String::from("<html><body>");
    for i in 0..TILES {
        html.push_str(&format!("<div style='left: {i}px; top: 2px; opacity: 0.5'></div>"));
    }
    html.push_str("</body></html>");
    b.set_document(Rc::new(Document::parse_html(&html).unwrap())).unwrap();
    b.evaluate(
        "var tiles = Array.prototype.slice.call(document.querySelectorAll('div'));          var wm = new WeakMap(), mp = new Map(); tiles.forEach(function (t, i) { wm.set(t, i); mp.set(t, i); });          var px = new Proxy({}, { get: function (t, p) { return p; } });",
    )
    .unwrap();
    b
}

fn parses(b: &DomBindings) -> f64 {
    match b.runtime.borrow_mut().evaluate_host_bookkeeping("return __rk.inlineStyleParses()").unwrap() {
        JsValue::Number(n) => n,
        other => panic!("{other:?}"),
    }
}

/// Runs `body` (a script using `i`, the tile `d` and the operation count `k`) `ops` times in one
/// turn, after one warm-up pass of `min(ops, 500)` in a turn of its own, and returns microseconds per
/// operation and inline style attribute parses per operation in the timed pass.
fn per_op(b: &DomBindings, ops: usize, body: &str) -> (f64, f64) {
    let op = format!("function op(k) {{ var i = k % {TILES}, d = tiles[i]; {body} }}");
    b.evaluate(&format!("(function () {{ {op} for (var k = 0; k < {}; k++) op(k); }})()", ops.min(500)))
        .unwrap();
    let before = parses(b);
    let script = format!(
        "(function () {{ {op} var t0 = performance.now(); for (var k = 0; k < {ops}; k++) op(k); \
           return String((performance.now() - t0) * 1000 / {ops}); }})()"
    );
    let us = match b.evaluate(&script).unwrap() {
        JsValue::String(s) => s.parse().unwrap(),
        other => panic!("{other:?}"),
    };
    (us, (parses(b) - before) / ops as f64)
}

#[test]
#[ignore]
fn measure_inline_style_write_stages() {
    let b = page();
    let n = 15000;
    let rows: Vec<(&str, (f64, f64))> = vec![
        ("empty host call (floor)", per_op(&b, n, "__style1b_noop();")),
        ("d.nodeType (one wrapped host read)", per_op(&b, n, "d.nodeType;")),
        ("d.getAttribute('style')", per_op(&b, n, "d.getAttribute('style');")),
        (
            "d.setAttribute('style', text), changing",
            per_op(&b, n, "d.setAttribute('style', 'left: ' + k + 'px; top: 2px; opacity: 0.5');"),
        ),
        ("d.style (the element's Proxy, fetched)", per_op(&b, n, "d.style;")),
        ("d.style.left read", per_op(&b, n, "d.style.left;")),
        ("d.style.left read, one tile", per_op(&b, n, "tiles[0].style.left;")),
        ("plain JS: WeakMap get, 5000 element keys", per_op(&b, n, "wm.get(d);")),
        ("plain JS: WeakMap get, one key", per_op(&b, n, "wm.get(tiles[0]);")),
        ("plain JS: Map get, 5000 element keys", per_op(&b, n, "mp.get(d);")),
        ("plain JS: Proxy get trap", per_op(&b, n, "px.left;")),
        ("d.style.left = same (no-op)", per_op(&b, n, "d.style.left = d.style.left;")),
        ("d.style.left = v, changing", per_op(&b, n, "d.style.left = (k % 97) + 'px';")),
        (
            "Multiply frame: 3 writes per tile (per write)",
            per_op(
                &b,
                n,
                "var j = k % 3; if (j === 0) d.style.left = (k % 97) + 'px'; \
                 else if (j === 1) d.style.top = (k % 89) + 'px'; else d.style.opacity = String((k % 10) / 10);",
            ),
        ),
    ];
    // The same writes with a MutationObserver watching the body's attributes.
    b.evaluate(
        "var mo = new MutationObserver(function () {}); \
         mo.observe(document.body, { attributes: true, attributeOldValue: true, subtree: true });",
    )
    .unwrap();
    let observed = per_op(&b, n, "d.style.left = (k % 97) + 'px';");
    b.evaluate("mo.takeRecords(); mo.disconnect();").unwrap();
    let again = per_op(&b, n, "d.setAttribute('style', 'left: ' + k + 'px; top: 2px; opacity: 0.5');");
    let observed = ("d.style.left = v with a MutationObserver", observed);
    eprintln!("STYLE1B-COST d.setAttribute('style', text) again, after the style writes: {:.2} us/op", again.0);
    // Is the write path's cost its allocations (Boa's collector runs on allocation)? The same plain
    // allocation the style write makes per call (a list copy of three declarations), without the DOM.
    let alloc = per_op(
        &b,
        n,
        "var l = Object.create(null); l.n = 3; for (var j = 0; j < 3; j++) { var x = Object.create(null);          x.name = 'left'; x.value = k + 'px'; x.important = false; l[j] = x; }",
    );
    eprintln!("STYLE1B-COST plain JS: allocate a 3-declaration list: {:.2} us/op", alloc.0);
    let both = per_op(
        &b,
        n,
        "var l = Object.create(null); l.n = 3; for (var j = 0; j < 3; j++) { var x = Object.create(null);          x.name = 'left'; x.value = k + 'px'; x.important = false; l[j] = x; }          d.setAttribute('style', 'left: ' + k + 'px; top: 2px; opacity: 0.5');",
    );
    eprintln!("STYLE1B-COST that allocation + d.setAttribute('style', text): {:.2} us/op", both.0);
    for (name, (us, p)) in rows.iter().chain(std::iter::once(&observed)) {
        eprintln!("STYLE1B-COST {name}: {us:.2} us/op, {:.1} ms per 15000, {p:.2} parses/op", us * 15.0);
    }
}
