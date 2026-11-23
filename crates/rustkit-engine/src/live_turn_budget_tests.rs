//! A script that does not end, started after the load, is stopped (I0(a),
//! step M0 of docs/design/THREAD_OWNERSHIP_2026-10.md).
//!
//! The load's scripts have had a wall-clock budget since 2026-10-08. What
//! runs afterwards had none: a timer callback or an event listener inside
//! nested `forEach` callbacks (no loop statement, so the loop limit does not
//! count it) held the window's thread for as long as it ran. Real window,
//! 2026-10-12: `i0a_script` held the window thread for as long as the check
//! watched. This bounds the hold; it does not remove it.

use super::*;
use crate::script_net_tests::serve_routes;

/// About nine million callbacks, with a host call every 4096: far longer
/// than the budgets below, and stoppable at a host call.
const SPIN: &str = "window.started = true; var spun = 0; var row = new Array(3000).fill(0);
row.forEach(function () { row.forEach(function () { spun++; if (spun % 4096 === 0) { document.getElementById('p'); } }); });";

fn config() -> EngineConfig {
    EngineConfig {
        interrupt_scripts_at_budget: true,
        live_turn_budget_ms: 200,
        ..EngineConfig::default()
    }
}

fn load(config: EngineConfig, page: String) -> (Engine, EngineViewId) {
    let server = serve_routes(vec![("/", "text/html", page)]);
    let mut engine = Engine::new(config).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 200, height: 100 })
        .expect("view");
    let url = Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap();
    runtime().block_on(engine.load_url(view, url)).expect("load_url");
    (engine, view)
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap()
}

fn over_budget(engine: &Engine, view: EngineViewId) -> Vec<String> {
    engine
        .script_log(view)
        .unwrap()
        .iter()
        .filter(|record| record.outcome == ScriptOutcome::OverBudget)
        .map(|record| record.source.clone())
        .collect()
}

/// Both timers are due after the load's own timer horizon, so the load
/// leaves them for the live loop.
fn timer_page() -> String {
    format!(
        "<html><body><p id=p>alive</p><script>
setTimeout(function () {{ {SPIN} }}, 60000);
setTimeout(function () {{ window.after = true; document.title = 'after'; }}, 61000);
</script></body></html>"
    )
}

#[test]
fn a_timer_callback_still_running_at_the_turn_budget_is_stopped() {
    let (mut engine, view) = load(config(), timer_page());
    assert!(over_budget(&engine, view).is_empty(), "the load ran neither timer");
    let started = std::time::Instant::now();
    runtime().block_on(engine.pump_live(view, 60_000));
    let took = started.elapsed();
    assert!(took < std::time::Duration::from_millis(1500), "the callback ran on past the turn budget: {took:?}");
    assert_eq!(over_budget(&engine, view), ["timers"], "{:#?}", engine.script_log(view));
    // The page is still a page: its next timer runs on a later turn, and
    // script runs in it.
    runtime().block_on(engine.pump_live(view, 1_000));
    assert_eq!(engine.execute_script(view, "window.after === true && window.started === true").unwrap(), "Boolean(true)");
    assert_eq!(engine.execute_script(view, "document.title").unwrap(), r#"String("after")"#);
}

#[test]
fn a_click_listener_still_running_at_the_turn_budget_is_stopped() {
    let page = format!(
        "<html><body style=\"margin:0\"><p id=p style=\"margin:0;height:100px\">alive</p><script>
document.getElementById('p').addEventListener('click', function () {{ {SPIN} }});
</script></body></html>"
    );
    let (mut engine, view) = load(config(), page);
    let started = std::time::Instant::now();
    engine.click_at_point(view, 20.0, 20.0);
    let took = started.elapsed();
    assert!(took < std::time::Duration::from_millis(1500), "the listener ran on past the turn budget: {took:?}");
    assert_eq!(over_budget(&engine, view), ["event:click"], "{:#?}", engine.script_log(view));
    assert_eq!(engine.execute_script(view, "document.title = 'x'; window.started === true").unwrap(), "Boolean(true)");
}

/// The board's frames are taken with scripts that run to their end.
#[test]
fn without_the_switch_a_live_turn_has_no_budget() {
    let page = "<html><body><p id=p>alive</p><script>
setTimeout(function () { var n = 0; var row = new Array(150).fill(0);
  row.forEach(function () { row.forEach(function () { n++; document.getElementById('p'); }); }); window.n = n; }, 60000);
</script></body></html>";
    let config = EngineConfig { live_turn_budget_ms: 1, ..EngineConfig::default() };
    assert!(!config.interrupt_scripts_at_budget);
    let (mut engine, view) = load(config, page.to_string());
    runtime().block_on(engine.pump_live(view, 60_000));
    assert!(over_budget(&engine, view).is_empty(), "{:#?}", engine.script_log(view));
    assert_eq!(engine.execute_script(view, "window.n").unwrap(), "Number(22500.0)");
}

fn load_routes(config: EngineConfig, routes: Vec<(&'static str, &'static str, String)>) -> (Engine, EngineViewId) {
    let server = serve_routes(routes);
    let mut engine = Engine::new(config).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 200, height: 100 })
        .expect("view");
    let url = Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap();
    runtime().block_on(engine.load_url(view, url)).expect("load_url");
    (engine, view)
}

/// A few more turns of the loop, 50 ms apart, on one runtime (a request
/// lives on the runtime that started it).
fn turns(rt: &tokio::runtime::Runtime, engine: &mut Engine, view: EngineViewId, count: usize) {
    for _ in 0..count {
        std::thread::sleep(std::time::Duration::from_millis(50));
        rt.block_on(engine.pump_live(view, 50));
    }
}

/// One timer asks for `/data`; another, due in the same turn, does not end.
/// `spin` empty is the control.
fn answer_page(spin: &str) -> String {
    format!(
        "<html><body><p id=p>alive</p><script>
window.got = 'none';
setTimeout(function () {{}}, 60000);
setTimeout(function () {{
  var x = window.x = new XMLHttpRequest(); x.open('GET', '/data');
  x.onload = function () {{ document.getElementById('p').textContent = x.responseText; window.got = x.responseText; }};
  x.onerror = function () {{ window.got = 'error'; }};
  x.send();
}}, 61000);
setTimeout(function () {{ {spin} }}, 61000);
</script></body></html>"
    )
}

const GOT: &str = "window.got + ' ' + document.getElementById('p').textContent + ' rs=' + window.x.readyState";

/// Review of #666 (R1 HOLD @ 8e4e5c22): the answer came in the turn whose
/// timer was stopped, the bridge forgot the request before it called
/// `onload`, `onload`'s first host call was refused, and the request never
/// finished: no `load`, no `error`, on any later turn.
#[test]
fn an_answer_that_arrives_in_a_stopped_turn_is_delivered_on_a_later_one() {
    for (spin, stops) in [("", 0), (SPIN, 1)] {
        let routes = vec![("/", "text/html", answer_page(spin)), ("/data", "text/plain", "hello".to_string())];
        let (mut engine, view) = load_routes(config(), routes);
        let rt = runtime();
        rt.block_on(engine.pump_live(view, 60_000));
        rt.block_on(engine.pump_live(view, 1_000));
        turns(&rt, &mut engine, view, 10);
        assert_eq!(over_budget(&engine, view).len(), stops, "{:#?}", engine.script_log(view));
        assert_eq!(
            engine.execute_script(view, GOT).unwrap(),
            r#"String("hello hello rs=4")"#,
            "spin={:?} {:#?}",
            !spin.is_empty(),
            engine.script_log(view)
        );
    }
}

/// The same loss one step later: two answers arrive in one turn and the
/// first one's handler does not end. The second is not handed to a turn
/// that has no budget left.
#[test]
fn an_answer_behind_a_stopped_response_handler_is_delivered_on_a_later_turn() {
    let page = format!(
        "<html><body><p id=p>alive</p><script>
window.got = 'none';
setTimeout(function () {{
  var a = new XMLHttpRequest(); a.open('GET', '/held-first'); a.onload = function () {{ {SPIN} }}; a.send();
  var b = new XMLHttpRequest(); b.open('GET', '/held-data');
  b.onload = function () {{ document.getElementById('p').textContent = b.responseText; window.got = b.responseText; }};
  b.send();
}}, 60000);
</script></body></html>"
    );
    let routes = vec![
        ("/", "text/html", page),
        ("/held-first", "text/plain", "first".to_string()),
        ("/held-data", "text/plain", "hello".to_string()),
    ];
    let (mut engine, view) = load_routes(config(), routes);
    let rt = runtime();
    rt.block_on(engine.pump_live(view, 60_000));
    // Both answers are in before the turn that hands them over (left to
    // the loop they arrive a turn apart, each with a budget of its own).
    let out = std::mem::take(&mut engine.views.get_mut(&view).unwrap().live_requests);
    assert_eq!(out.len(), 2);
    let answers = rt.block_on(futures::future::join_all(out));
    engine.views.get_mut(&view).unwrap().live_requests = answers
        .into_iter()
        .map(|answer| Box::pin(std::future::ready(answer)) as script_net::LiveRequest)
        .collect();
    turns(&rt, &mut engine, view, 12);
    assert_eq!(over_budget(&engine, view), ["network"], "{:#?}", engine.script_log(view));
    assert_eq!(
        engine.execute_script(view, "window.started + ' ' + window.got").unwrap(),
        r#"String("true hello")"#,
        "{:#?}",
        engine.script_log(view)
    );
}

#[test]
fn an_input_listener_still_running_at_the_turn_budget_is_stopped() {
    let page = format!(
        "<html><body><p id=p>alive</p><input id=q><script>
document.getElementById('q').addEventListener('input', function () {{ {SPIN} }});
</script></body></html>"
    );
    let (mut engine, view) = load(config(), page);
    engine.execute_script(view, "document.getElementById('q').focus()").unwrap();
    let started = std::time::Instant::now();
    engine.handle_text_key(view, 0, "a", false, false, false);
    let took = started.elapsed();
    assert!(took < std::time::Duration::from_millis(1500), "the listener ran on past the turn budget: {took:?}");
    assert_eq!(over_budget(&engine, view), ["event:input"], "{:#?}", engine.script_log(view));
    assert_eq!(
        engine.execute_script(view, "window.started + ' ' + document.getElementById('q').value").unwrap(),
        r#"String("true a")"#
    );
}

#[test]
fn a_resize_listener_still_running_at_the_turn_budget_is_stopped() {
    let page = format!(
        "<html><body><p id=p>alive</p><script>
window.addEventListener('resize', function () {{ {SPIN} }});
</script></body></html>"
    );
    let (mut engine, view) = load(config(), page);
    engine.set_view_bounds(view, Bounds { x: 0, y: 0, width: 300, height: 100 }).unwrap();
    let started = std::time::Instant::now();
    runtime().block_on(engine.pump_live(view, 16));
    let took = started.elapsed();
    assert!(took < std::time::Duration::from_millis(1500), "the listener ran on past the turn budget: {took:?}");
    assert_eq!(over_budget(&engine, view), ["event:resize"], "{:#?}", engine.script_log(view));
    assert_eq!(engine.execute_script(view, "window.started + ' ' + window.innerWidth").unwrap(), r#"String("true 300")"#);
}

/// A stop is one record. Until this test the script log also carried the
/// error that unwinds the stopped script, which reads as the loop limit
/// ("reached the maximum number of iteration loops"), and the page's own
/// console said nothing.
#[test]
fn a_stopped_turn_is_one_record_and_a_line_on_the_page_console() {
    let page = format!(
        "<html><body style=\"margin:0\"><p id=p style=\"margin:0;height:100px\">alive</p><script>
window.warned = []; console.warn = function (message) {{ window.warned.push(String(message)); }};
setTimeout(function () {{ {SPIN} }}, 60000);
document.getElementById('p').addEventListener('click', function () {{ {SPIN} }});
</script></body></html>"
    );
    let (mut engine, view) = load(config(), page);
    runtime().block_on(engine.pump_live(view, 60_000));
    engine.click_at_point(view, 20.0, 20.0);
    let log: Vec<String> = engine
        .script_log(view)
        .unwrap()
        .iter()
        .filter(|record| record.outcome != ScriptOutcome::Ran)
        .map(|record| format!("{} {:?}", record.source, record.outcome))
        .collect();
    assert_eq!(log, ["timers OverBudget", "event:click OverBudget"]);
    assert_eq!(
        engine.execute_script(view, "window.warned.join('|')").unwrap(),
        r#"String("Script stopped: it ran past 200 ms (timers)|Script stopped: it ran past 200 ms (event:click)")"#
    );
}

/// The budget is per EVENT, not per user action (the CLEAR's first
/// non-blocking note, T0 of the threads plan): one click is a press, a
/// release and a click, each its own turn with its own budget, so a page
/// whose three handlers each do not end holds the window three budgets,
/// not one. Pinned so the figure is known: with the 5 s budget that is 15 s
/// for one click.
#[test]
fn one_click_whose_three_handlers_each_spin_is_stopped_three_times() {
    let page = format!(
        "<html><body style=\"margin:0\"><p id=p style=\"margin:0;height:100px\">alive</p><script>
var p = document.getElementById('p');
p.addEventListener('pointerdown', function () {{ window.pd = true; }});
p.addEventListener('mousedown', function () {{ {SPIN} }});
p.addEventListener('mouseup', function () {{ {SPIN} }});
p.addEventListener('click', function () {{ {SPIN} }});
</script></body></html>"
    );
    let (mut engine, view) = load(config(), page);
    let started = std::time::Instant::now();
    // The user's click: the press (pointerdown, mousedown), then the
    // release (pointerup, mouseup, click).
    engine.mouse_down_at_point(view, 20.0, 20.0);
    engine.click_at_point(view, 20.0, 20.0);
    let took = started.elapsed();
    assert_eq!(
        over_budget(&engine, view),
        ["event:mousedown", "event:mouseup", "event:click"],
        "{:#?}",
        engine.script_log(view)
    );
    assert_eq!(engine.execute_script(view, "window.pd === true").unwrap(), "Boolean(true)", "pointerdown was heard");
    assert!(
        took >= std::time::Duration::from_millis(600) && took < std::time::Duration::from_millis(3000),
        "three budgets of 200 ms, each stopped at a host call: {took:?}"
    );
    // The page is still a page.
    assert_eq!(engine.execute_script(view, "document.title = 'x'; window.started === true").unwrap(), "Boolean(true)");
}

/// An interval whose callback never ends is stopped on every turn it runs
/// in, and runs again on the next turn (the CLEAR's second non-blocking
/// note): the budget bounds each hold, it does not take the interval off
/// the page. A timeout due beside it is pinned too, so what such a page
/// starves is on record.
#[test]
fn an_interval_that_overruns_every_time_is_stopped_every_time() {
    let page = format!(
        "<html><body><p id=p>alive</p><script>
window.ticks = 0; window.after = false;
setTimeout(function () {{
  setInterval(function () {{ window.ticks++; {SPIN} }}, 10);
  setTimeout(function () {{ window.after = true; }}, 30);
}}, 60000);
</script></body></html>"
    );
    let (mut engine, view) = load(config(), page);
    let rt = runtime();
    rt.block_on(engine.pump_live(view, 60_000));
    let started = std::time::Instant::now();
    turns(&rt, &mut engine, view, 4);
    let took = started.elapsed();
    let stops = over_budget(&engine, view);
    assert!(stops.len() >= 3, "the interval ran and was stopped on each turn: {stops:?} {:#?}", engine.script_log(view));
    assert!(stops.iter().all(|s| s == "timers"), "{stops:?}");
    assert!(took < std::time::Duration::from_millis(4 * 500), "each turn held at most one budget: {took:?}");
    let ticks = engine.execute_script(view, "window.ticks").unwrap();
    assert_eq!(ticks, format!("Number({}.0)", stops.len()), "one callback start per stop");
    // The timeout due 30 ms after the interval started: whether it runs
    // beside an interval that overruns every turn.
    assert_eq!(
        engine.execute_script(view, "window.after").unwrap(),
        "Boolean(true)",
        "the later timeout is starved by the overrunning interval: {:#?}",
        engine.script_log(view)
    );
}

// ── The stop apart from the allowance (Pete, 2026-11-21) ────────────────────
//
// Pete's decision is about when a script is stopped (30 s), not how much
// script one turn runs before it yields to rendering: the turn's allowance
// (`live_turn_budget_ms`) keeps pacing frames as before, and the stop
// (`live_script_stop_ms`) is its own number. Scaled down here: allowance
// 200 ms, stop 1500 ms.

fn split_config() -> EngineConfig {
    EngineConfig {
        interrupt_scripts_at_budget: true,
        live_turn_budget_ms: 200,
        live_script_stop_ms: Some(1_500),
        ..EngineConfig::default()
    }
}

/// Spins `ms` of wall clock, then says it finished.
fn busy(ms: u64) -> String {
    format!("window.began = true; var t = Date.now(); while (Date.now() - t < {ms}) {{}} window.finished = true;")
}

/// A callback that runs past the turn's allowance but not to the stop runs
/// to its end and is not stopped; the timer due beside it still waits for
/// the next turn (the allowance is spent), so pacing is unchanged.
#[test]
fn a_callback_past_the_allowance_runs_on_to_the_stop_and_the_next_waits_a_turn() {
    let page = format!(
        "<html><body><p id=p>alive</p><script>
window.second = 0;
setTimeout(function () {{ {} }}, 60000);
setTimeout(function () {{ window.second = window.finished ? 1 : -1; }}, 60000);
</script></body></html>",
        busy(500)
    );
    let (mut engine, view) = load(split_config(), page);
    let rt = runtime();
    rt.block_on(engine.pump_live(view, 60_000));
    assert!(over_budget(&engine, view).is_empty(), "stopped before the stop: {:#?}", engine.script_log(view));
    assert_eq!(engine.execute_script(view, "window.finished === true").unwrap(), "Boolean(true)");
    assert_eq!(
        engine.execute_script(view, "window.second").unwrap(),
        "Number(0.0)",
        "the second callback ran in the turn whose allowance the first had spent"
    );
    rt.block_on(engine.pump_live(view, 10));
    assert_eq!(engine.execute_script(view, "window.second").unwrap(), "Number(1.0)");
}

/// A callback that does not end is still stopped, at the stop and not at the
/// allowance.
#[test]
fn a_callback_that_does_not_end_is_stopped_at_the_stop_not_the_allowance() {
    let (mut engine, view) = load(split_config(), timer_page());
    let started = std::time::Instant::now();
    runtime().block_on(engine.pump_live(view, 60_000));
    let took = started.elapsed();
    assert_eq!(over_budget(&engine, view), ["timers"], "{:#?}", engine.script_log(view));
    assert!(took >= std::time::Duration::from_millis(1_400), "stopped at the allowance, not the stop: {took:?}");
    assert!(took < std::time::Duration::from_millis(4_000), "ran on past the stop: {took:?}");
}

/// An input event's listeners get the stop as well.
#[test]
fn a_click_listener_past_the_allowance_runs_on_to_the_stop() {
    let page = format!(
        "<html><body style=\"margin:0\"><p id=p style=\"margin:0;height:100px\">alive</p><script>
document.getElementById('p').addEventListener('click', function () {{ {} }});
</script></body></html>",
        busy(500)
    );
    let (mut engine, view) = load(split_config(), page);
    engine.click_at_point(view, 20.0, 20.0);
    assert!(over_budget(&engine, view).is_empty(), "{:#?}", engine.script_log(view));
    assert_eq!(engine.execute_script(view, "window.finished === true").unwrap(), "Boolean(true)");
}

/// The engine's default is unchanged: no separate stop, so the stop is the
/// allowance (parity-capture without `--interrupt-scripts`, the board, and
/// every test that sets only `live_turn_budget_ms`).
#[test]
fn by_default_the_stop_is_the_allowance() {
    let config = EngineConfig::default();
    assert_eq!(config.live_turn_budget_ms, 5_000);
    assert_eq!(config.live_script_stop_ms, None);
}
