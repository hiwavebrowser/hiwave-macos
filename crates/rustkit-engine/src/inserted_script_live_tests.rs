//! Scripts a page inserts after its load are fetched across live turns.
//!
//! Field report, 2026-10 (a Next.js news feed): the ~20 scripts the page
//! inserted after load (its chunks and ad scripts) all failed "timed out" at
//! the same instant, 360-630 ms after they were asked for. The live turn
//! wrapped every inserted script's fetch in the turn's own two-second slice
//! (`LIVE_NETWORK_BUDGET`), shared by every round of that turn, so a chunk
//! inserted by a script that took a second to arrive had the rest of that
//! second. Each failed for good with `error`, the chunk loader rejected, and
//! the page reloaded itself. A script whose bytes did arrive, but in a turn
//! whose JavaScript budget was spent, was dropped with no event at all, so a
//! loader waiting on it waited forever.
//!
//! A turn's slice is scheduling, not a network timeout: a fetch stays out
//! across turns until it answers or the per-request timeout passes, the
//! script runs on the turn its bytes are in, and every inserted script hears
//! exactly one `load` or `error`.

use super::*;
use crate::script_net_tests::serve_routes;
use std::time::{Duration, Instant};

/// About nine million callbacks, with a host call every 4096: far longer
/// than the budgets below, and stoppable at a host call.
const SPIN: &str = "var spun = 0; var row = new Array(3000).fill(0);
row.forEach(function () { row.forEach(function () { spun++; if (spun % 4096 === 0) { document.getElementById('p'); } }); });";

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap()
}

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

struct Page {
    engine: Engine,
    view: EngineViewId,
    rt: tokio::runtime::Runtime,
    _server: crate::script_net_tests::Server,
}

impl Page {
    fn load(config: EngineConfig, routes: Vec<(&'static str, &'static str, String)>) -> Self {
        let server = serve_routes(routes);
        let mut engine = Engine::new(config).expect("engine");
        let view = engine
            .create_headless_view(Bounds { x: 0, y: 0, width: 200, height: 100 })
            .expect("view");
        let url = Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap();
        // One runtime for the load and every turn after it: a fetch lives
        // on the runtime that started it, as in the app.
        let rt = runtime();
        rt.block_on(engine.load_url(view, url)).expect("load_url");
        Page { engine, view, rt, _server: server }
    }

    fn js(&mut self, script: &str) -> String {
        self.engine.execute_script(self.view, script).unwrap()
    }

    /// One live turn; returns how long it held the thread.
    fn turn(&mut self, elapsed_ms: u64) -> Duration {
        let started = Instant::now();
        self.rt.block_on(self.engine.pump_live(self.view, elapsed_ms));
        started.elapsed()
    }

    /// Turns 20 ms apart, the way the app's loop comes back while work is
    /// out, until `done` reads `true` or `within` has passed, then `extra`
    /// more (anything owed twice would show by then). Returns the longest
    /// turn.
    fn turns_until(&mut self, done: &str, within: Duration, extra: usize) -> Duration {
        let started = Instant::now();
        let mut longest = Duration::ZERO;
        while started.elapsed() < within {
            std::thread::sleep(Duration::from_millis(20));
            longest = longest.max(self.turn(20));
            if self.js(done) == "Boolean(true)" {
                break;
            }
        }
        for _ in 0..extra {
            std::thread::sleep(Duration::from_millis(20));
            longest = longest.max(self.turn(20));
        }
        longest
    }

    fn log(&self) -> String {
        format!("{:#?}", self.engine.script_log(self.view))
    }
}

/// A script that takes 3 s to answer, longer than a turn's two-second
/// slice, inserted by a timer after the load.
#[test]
fn an_inserted_script_slower_than_a_turn_still_runs_and_hears_load() {
    let page = "<html><body><p id=p>x</p><script>
window.heard = [];
setTimeout(function () {
  var s = document.createElement('script');
  s.src = '/late.js?delay=3000';
  s.onload = function () { heard.push('load'); };
  s.onerror = function () { heard.push('error'); };
  document.head.appendChild(s);
}, 60000);
</script></body></html>";
    let routes = vec![
        ("/", "text/html", page.to_string()),
        ("/late.js?delay=3000", "text/javascript", "window.ran = (window.ran || 0) + 1;".to_string()),
    ];
    let mut page = Page::load(EngineConfig::default(), routes);
    let first = page.turn(60_000);
    let longest = page
        .turns_until("window.heard.length > 0", Duration::from_secs(8), 10)
        .max(first);
    assert_eq!(
        page.js("window.heard.join() + ' ran=' + window.ran"),
        r#"String("load ran=1")"#,
        "{}",
        page.log()
    );
    // No turn waits for the script: the window gets a turn in between.
    assert!(longest < Duration::from_millis(1000), "a turn held the thread {longest:?}");
}

/// Next.js-style: a timer inserts the webpack runtime, which takes 1 s, and
/// the runtime inserts 15 chunks that take 0.5-1.5 s each and rejects on
/// `error` (or on a `load` whose chunk did not register), plus three
/// `async = false` scripts that must run in insertion order.
#[test]
fn chunks_a_slow_loader_inserts_all_run_in_the_order_required() {
    let delays: Vec<u64> = (0..15).map(|i| 1500 - i * 71).collect();
    let ordered = [1200u64, 700, 200];
    let runtime_js = format!(
        "(function () {{
  var installed = {{}};
  self.ranChunks = [];
  self.order = [];
  self.orderedLoads = 0;
  self.chunkErrors = 0;
  self.chunkPush = function (id) {{ installed[id] = true; self.ranChunks.push(id); }};
  function load(id, delay) {{
    return new Promise(function (resolve, reject) {{
      var s = document.createElement('script');
      s.src = '/chunk-' + id + '.js?delay=' + delay;
      s.onload = function () {{ installed[id] ? resolve() : reject(new Error('chunk ' + id + ' missing')); }};
      s.onerror = function () {{ self.chunkErrors++; reject(new Error('chunk ' + id + ' failed')); }};
      document.head.appendChild(s);
    }});
  }}
  Promise.all({delays:?}.map(function (d, i) {{ return load(i, d); }})).then(
    function () {{ self.done = 'ok'; }},
    function (e) {{ self.done = 'error: ' + e.message; }});
  {ordered:?}.forEach(function (d, i) {{
    var s = document.createElement('script');
    s.async = false;
    s.src = '/ordered-' + i + '.js?delay=' + d;
    s.onload = function () {{ self.orderedLoads++; }};
    document.head.appendChild(s);
  }});
}})();"
    );
    let page = "<html><body><p id=p>x</p><script>
setTimeout(function () {
  var s = document.createElement('script');
  s.src = '/runtime.js?delay=1000';
  document.head.appendChild(s);
}, 60000);
</script></body></html>";
    let mut routes = vec![
        ("/", "text/html", page.to_string()),
        ("/runtime.js?delay=1000", "text/javascript", runtime_js),
    ];
    for (i, d) in delays.iter().enumerate() {
        routes.push((leak(format!("/chunk-{i}.js?delay={d}")), "text/javascript", format!("self.chunkPush({i});")));
    }
    for (i, d) in ordered.iter().enumerate() {
        routes.push((leak(format!("/ordered-{i}.js?delay={d}")), "text/javascript", format!("self.order.push({i});")));
    }
    let mut page = Page::load(EngineConfig::default(), routes);
    page.turn(60_000);
    let longest = page.turns_until(
        "window.done !== undefined && window.orderedLoads === 3",
        Duration::from_secs(10),
        5,
    );
    assert_eq!(
        page.js("window.done + ' errors=' + window.chunkErrors + ' order=' + window.order.join() + ' loads=' + window.orderedLoads"),
        r#"String("ok errors=0 order=0,1,2 loads=3")"#,
        "{}",
        page.log()
    );
    // Each chunk ran once; async ones as they arrived (the last asked for
    // answers a second before the first).
    assert_eq!(page.js("window.ranChunks.length"), "Number(15.0)");
    assert_eq!(
        page.js("window.ranChunks.indexOf(14) < window.ranChunks.indexOf(0)"),
        "Boolean(true)",
        "{}",
        page.js("window.ranChunks.join()")
    );
    assert!(longest < Duration::from_millis(1000), "a turn held the thread {longest:?}");
}

fn budget_config() -> EngineConfig {
    EngineConfig {
        interrupt_scripts_at_budget: true,
        live_turn_budget_ms: 200,
        ..EngineConfig::default()
    }
}

/// Two `async = false` scripts that answer at once: the first does not end
/// and is stopped at the turn's budget, so the second's bytes are in when
/// the turn has no JavaScript left. It runs on a later turn, and each
/// element hears `load` exactly once.
#[test]
fn an_inserted_script_that_arrives_after_the_turn_budget_runs_on_a_later_turn() {
    let page = "<html><body><p id=p>x</p><script>
window.heard = [];
setTimeout(function () {
  ['spin', 'late'].forEach(function (name) {
    var s = document.createElement('script');
    s.async = false;
    s.src = '/' + name + '.js';
    s.onload = function () { heard.push(name + ':load'); };
    s.onerror = function () { heard.push(name + ':error'); };
    document.head.appendChild(s);
  });
}, 60000);
</script></body></html>";
    let routes = vec![
        ("/", "text/html", page.to_string()),
        ("/spin.js", "text/javascript", SPIN.to_string()),
        ("/late.js", "text/javascript", "window.lateRan = (window.lateRan || 0) + 1;".to_string()),
    ];
    let mut page = Page::load(budget_config(), routes);
    page.turn(60_000);
    page.turns_until("window.heard.length >= 2", Duration::from_secs(8), 10);
    assert_eq!(
        page.js("window.heard.join() + ' lateRan=' + window.lateRan"),
        r#"String("spin:load,late:load lateRan=1")"#,
        "{}",
        page.log()
    );
}

/// The same during the load: the load's own script budget is spent by the
/// first inserted script, and the second, already fetched, is handed to the
/// live loop rather than dropped.
#[test]
fn an_inserted_script_fetched_after_the_load_budget_runs_on_a_live_turn() {
    let page = "<html><body><p id=p>x</p><script>
window.heard = [];
['spin', 'late'].forEach(function (name) {
  var s = document.createElement('script');
  s.async = false;
  s.src = '/' + name + '.js';
  s.onload = function () { heard.push(name + ':load'); };
  s.onerror = function () { heard.push(name + ':error'); };
  document.head.appendChild(s);
});
</script></body></html>";
    let routes = vec![
        ("/", "text/html", page.to_string()),
        ("/spin.js", "text/javascript", SPIN.to_string()),
        ("/late.js", "text/javascript", "window.lateRan = (window.lateRan || 0) + 1;".to_string()),
    ];
    let config = EngineConfig { script_budget_ms: 300, ..budget_config() };
    let mut page = Page::load(config, routes);
    page.turns_until("window.heard.length >= 2", Duration::from_secs(8), 10);
    assert_eq!(
        page.js("window.heard.join() + ' lateRan=' + window.lateRan"),
        r#"String("spin:load,late:load lateRan=1")"#,
        "{}",
        page.log()
    );
}

/// A script that really fails (a 404; a refused connection) still hears
/// `error`, exactly once.
#[test]
fn an_inserted_script_that_fails_hears_error_exactly_once() {
    let refused = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let page = format!(
        "<html><body><p id=p>x</p><script>
window.heard = [];
setTimeout(function () {{
  [['missing', '/missing.js'], ['refused', 'http://127.0.0.1:{refused}/gone.js']].forEach(function (p) {{
    var s = document.createElement('script');
    s.src = p[1];
    s.onload = function () {{ heard.push(p[0] + ':load'); }};
    s.onerror = function () {{ heard.push(p[0] + ':error'); }};
    document.head.appendChild(s);
  }});
}}, 60000);
</script></body></html>"
    );
    let mut page = Page::load(EngineConfig::default(), vec![("/", "text/html", page)]);
    page.turn(60_000);
    page.turns_until("window.heard.length >= 2", Duration::from_secs(8), 10);
    assert_eq!(
        page.js("window.heard.slice().sort().join()"),
        r#"String("missing:error,refused:error")"#,
        "{}",
        page.log()
    );
}
