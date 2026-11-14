//! What a pointer move's hit test costs on a large page (R2 MED 6 on #1151). The timer wraps
//! `Engine::hit_at` only: stamping the sticky shifts and the layout's hit test (fixed boxes, then the
//! page in stacking order), with no event dispatch and no script. The page: 2000 `position: relative`
//! cards in one wrapper, each with an absolute badge, then a chain of 300 nested `position: relative`
//! boxes (the deep shape where a lifted box's path is long). A second, nested page (R3 LOW 8): 500 cards,
//! each under ten plain wrappers, and the same chain. Ignored: a measurement, run by hand with
//! `cargo test --profile parity -p rustkit-engine --features headless --lib hit_test_cost -- --ignored --nocapture`.

use super::*;
use std::time::Instant;

fn page(nested: bool) -> String {
    let mut body = String::from("<div class=wrap>");
    let (cards, depth) = if nested { (500, 10) } else { (2000, 0) };
    for i in 0..cards {
        body.push_str(&"<div class=nest>".repeat(depth));
        body.push_str(&format!(
            "<div class=card><div class=pic></div><span class=badge>{i}</span><p>card {i}</p></div>"
        ));
        body.push_str(&"</div>".repeat(depth));
    }
    body.push_str("</div>");
    for _ in 0..300 {
        body.push_str("<div class=chain>");
    }
    body.push_str("<div class=leaf>leaf</div>");
    for _ in 0..300 {
        body.push_str("</div>");
    }
    format!(
        "<!doctype html><html><head><style>\
         body{{margin:0;font:12px/16px sans-serif}}\
         .wrap{{display:flex;flex-wrap:wrap}}\
         .card{{position:relative;width:180px;height:120px;margin:4px;background:#eee}}\
         .pic{{height:80px;background:#ccd}}\
         .badge{{position:absolute;right:4px;top:4px;background:#c00;color:#fff}}\
         .chain{{position:relative;padding-left:1px}}\
         </style></head><body>{body}</body></html>"
    )
}

#[test]
#[ignore]
fn a_pointer_move_hit_test_on_a_large_page() {
    measure("flat", false);
    measure("nested", true);
}

fn measure(label: &str, nested: bool) {
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine.create_headless_view(Bounds::new(0, 0, 1280, 800)).expect("view");
    engine.load_html(view, &page(nested)).expect("load_html");
    // A frame is painted after a layout before a pointer can reach it, as in the app.
    engine.render_view(view).expect("render");
    let state = &engine.views[&view];
    let points: Vec<(f32, f32)> = (0..400).map(|i| ((i * 97 % 1280) as f32, (i * 61 % 800) as f32)).collect();

    // The first hit test after a layout and its frame also marks the boxes (`ensure_hit_overflow`); the marking walk
    // alone is timed on a second load of the same page.
    let started = Instant::now();
    let first = Engine::hit_at(state, 10.0, 10.0);
    let first_us = started.elapsed().as_secs_f64() * 1e6;
    assert!(first.is_some());
    let mark_us = {
        let mut other = Engine::new(EngineConfig::default()).expect("engine");
        let v = other.create_headless_view(Bounds::new(0, 0, 1280, 800)).expect("view");
        other.load_html(v, &page(nested)).expect("load_html");
        let layout = other.views[&v].layout.as_ref().expect("layout").clone();
        let started = Instant::now();
        layout.mark_hit_overflow();
        started.elapsed().as_secs_f64() * 1e6
    };

    let rounds = 5;
    let started = Instant::now();
    let (mut hits, mut sig) = (0usize, 0u64);
    for _ in 0..rounds {
        for &(x, y) in &points {
            if let Some(hit) = Engine::hit_at(state, x, y) {
                hits += 1;
                // Which node each point hits: the same sum on two builds means the same answers.
                sig = sig.wrapping_mul(31).wrapping_add(hit.node_id.unwrap_or(0) as u64);
            }
        }
    }
    let per_us = started.elapsed().as_secs_f64() * 1e6 / (rounds * points.len()) as f64;
    println!("HIT_COST {label} first_us={first_us:.0} mark_us={mark_us:.0} steady_us_per_hit={per_us:.1} hits={hits} sig={sig} n={}", rounds * points.len());
}
