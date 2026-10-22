//! A news feed that fills as the user scrolls, with the user scrolling
//! before or while it fills (the yahoo.com front page white after two flicks,
//! 2026-10-22). The trigger side: is the page's own code told that the user
//! scrolled and that rows are now in view (IntersectionObserver, `scroll`,
//! `requestAnimationFrame`, timers), and does what it then writes reach the
//! display list inside the viewport?
//!
//! Each fixture is a 400x200 viewport over a 30-row feed (150 px a row, so
//! 4500 px), scrolled by two 300 px flicks (1.5 viewports each) the way the
//! viewhost delivers them (`scroll_view`, negative delta downwards).

use super::*;
use crate::script_net_tests::serve_routes;
use rustkit_layout::DisplayCommand;

const VIEW_H: f32 = 200.0;
const FLICK: f32 = 300.0;

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap()
}

fn load(page: &str) -> (Engine, EngineViewId, crate::script_net_tests::Server) {
    let server = serve_routes(vec![("/", "text/html", page.to_string())]);
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 400, height: VIEW_H as u32 })
        .expect("view");
    let url = Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap();
    rt().block_on(engine.load_url(view, url)).expect("load_url");
    (engine, view, server)
}

fn read(engine: &mut Engine, view: EngineViewId, expr: &str) -> String {
    let value = engine.execute_script(view, &format!("String({expr})")).unwrap();
    value
        .strip_prefix("String(\"")
        .and_then(|v| v.strip_suffix("\")"))
        .unwrap_or(&value)
        .to_string()
}

/// Live turns of `step` ms until `total` ms have passed (the window loop).
fn pump(engine: &mut Engine, view: EngineViewId, total: u64, step: u64) {
    let rt = rt();
    let mut t = 0;
    while t < total {
        rt.block_on(engine.pump_live(view, step));
        t += step;
    }
}

fn flick(engine: &mut Engine, view: EngineViewId) {
    assert!(engine.scroll_view(view, 0.0, -FLICK).unwrap(), "the flick moved the view");
}

/// The text runs of the view's display list that fall inside the viewport at
/// its current scroll offset (document coordinates: scroll is a render-time
/// translate).
fn texts_in_view(engine: &Engine, view: EngineViewId) -> Vec<String> {
    let v = &engine.views[&view];
    let top = v.scroll_offset.1;
    let list = v.display_list.as_ref().expect("a display list");
    list.commands
        .iter()
        .filter_map(|c| match c {
            DisplayCommand::Text { text, y, .. } if *y >= top - 20.0 && *y <= top + VIEW_H => {
                Some(text.trim().to_string())
            }
            _ => None,
        })
        .filter(|t| !t.is_empty())
        .collect()
}

fn assert_feed_painted(engine: &mut Engine, view: EngineViewId, what: &str) {
    let y = engine.views[&view].scroll_offset.1;
    assert_eq!(y, 2.0 * FLICK, "{what}: two flicks down");
    let texts = texts_in_view(engine, view);
    let filled = read(engine, view, "document.querySelectorAll('.row.filled').length");
    assert!(
        texts.iter().any(|t| t.starts_with("story")),
        "{what}: the viewport at y={y} paints no feed row (filled rows: {filled}; texts in view: {texts:?})"
    );
}

/// 30 empty rows with their height reserved (a skeleton), the footer last.
fn feed(head: &str, rows: bool, script: &str) -> String {
    let rows = if rows {
        (0..30).map(|i| format!("<div class=row data-i={i}></div>")).collect::<String>()
    } else {
        String::new()
    };
    format!(
        "<html><head><style>body{{margin:0;font:16px sans-serif}} #feed{{min-height:4500px}} \
         .row{{height:150px}} {head}</style></head><body><div id=feed>{rows}</div>\
         <div id=footer>footer</div><script>{script}</script></body></html>"
    )
}

/// The row's story, the way a feed renders one.
const FILL: &str = "function fill(row) { if (row.classList.contains('filled')) return; \
    row.classList.add('filled'); var h = document.createElement('h3'); \
    h.textContent = 'story ' + row.getAttribute('data-i'); row.appendChild(h); }";

/// IntersectionObserver-driven lazy rows, observed at load.
#[test]
fn io_lazy_rows_fill_after_two_flicks() {
    let script = format!(
        "{FILL} var io = new IntersectionObserver(function (es) {{ es.forEach(function (e) {{ \
         if (e.isIntersecting) {{ fill(e.target); io.unobserve(e.target); }} }}); }}, {{ rootMargin: '0px 0px 100px 0px' }}); \
         document.querySelectorAll('.row').forEach(function (r) {{ io.observe(r); }});"
    );
    let (mut engine, view, _server) = load(&feed("", true, &script));
    flick(&mut engine, view);
    flick(&mut engine, view);
    assert_feed_painted(&mut engine, view, "IO rows observed at load");
}

/// The rows arrive after the load (a live timer, as from a feed fetch), and
/// the user flicked twice before they did. Rows are observed when inserted.
#[test]
fn io_rows_inserted_after_the_user_scrolled() {
    let script = format!(
        "{FILL} var io = new IntersectionObserver(function (es) {{ es.forEach(function (e) {{ \
         if (e.isIntersecting) {{ fill(e.target); io.unobserve(e.target); }} }}); }}); \
         setTimeout(function () {{ var feed = document.getElementById('feed'); \
           for (var i = 0; i < 30; i++) {{ var r = document.createElement('div'); r.className = 'row'; \
             r.setAttribute('data-i', i); feed.appendChild(r); io.observe(r); }} }}, 6000);"
    );
    let (mut engine, view, _server) = load(&feed("", false, &script));
    flick(&mut engine, view);
    flick(&mut engine, view);
    // The load's clock stops at its last timer; the live loop's runs on.
    pump(&mut engine, view, 6000, 100);
    pump(&mut engine, view, 200, 16);
    assert_eq!(read(&mut engine, view, "document.querySelectorAll('.row').length"), "30", "the feed arrived");
    assert_feed_painted(&mut engine, view, "IO rows inserted after the flicks");
}

/// The user flicks while the feed is being filled in batches by live timers.
#[test]
fn io_rows_inserted_in_batches_while_the_user_scrolls() {
    let script = format!(
        "{FILL} var io = new IntersectionObserver(function (es) {{ es.forEach(function (e) {{ \
         if (e.isIntersecting) {{ fill(e.target); io.unobserve(e.target); }} }}); }}); \
         var next = 0; function batch() {{ var feed = document.getElementById('feed'); \
           for (var k = 0; k < 5 && next < 30; k++, next++) {{ var r = document.createElement('div'); r.className = 'row'; \
             r.setAttribute('data-i', next); feed.appendChild(r); io.observe(r); }} \
           if (next < 30) setTimeout(batch, 50); }} \
         setTimeout(batch, 5500);"
    );
    let (mut engine, view, _server) = load(&feed("", false, &script));
    pump(&mut engine, view, 5500, 100);
    pump(&mut engine, view, 64, 16);
    flick(&mut engine, view);
    pump(&mut engine, view, 48, 16);
    flick(&mut engine, view);
    pump(&mut engine, view, 1000, 16);
    assert_eq!(read(&mut engine, view, "document.querySelectorAll('.row').length"), "30", "the feed arrived");
    assert_feed_painted(&mut engine, view, "IO rows inserted in batches across the flicks");
}

/// No observer: a `scroll` listener asks for an animation frame, which fills
/// the rows `getBoundingClientRect` puts in the viewport.
#[test]
fn scroll_listener_raf_fills_visible_rows() {
    let script = format!(
        "{FILL} var pending = false; function check() {{ pending = false; \
           document.querySelectorAll('.row').forEach(function (r) {{ var b = r.getBoundingClientRect(); \
             if (b.bottom > 0 && b.top < innerHeight) fill(r); }}); }} \
         window.addEventListener('scroll', function () {{ if (!pending) {{ pending = true; requestAnimationFrame(check); }} }}, {{ passive: true }}); \
         check();"
    );
    let (mut engine, view, _server) = load(&feed("", true, &script));
    flick(&mut engine, view);
    pump(&mut engine, view, 32, 16);
    flick(&mut engine, view);
    pump(&mut engine, view, 32, 16);
    assert_feed_painted(&mut engine, view, "scroll + rAF fill");
}

/// A throttled `scroll` listener (a timer, `Date.now()` for the throttle).
#[test]
fn throttled_scroll_listener_fills_visible_rows() {
    let script = format!(
        "{FILL} var last = 0, timer = null; function check() {{ timer = null; last = Date.now(); \
           document.querySelectorAll('.row').forEach(function (r) {{ var b = r.getBoundingClientRect(); \
             if (b.bottom > 0 && b.top < window.innerHeight) fill(r); }}); }} \
         document.addEventListener('scroll', function () {{ if (timer) return; \
           var wait = Math.max(0, 100 - (Date.now() - last)); timer = setTimeout(check, wait); }}); \
         check();"
    );
    let (mut engine, view, _server) = load(&feed("", true, &script));
    flick(&mut engine, view);
    flick(&mut engine, view);
    pump(&mut engine, view, 300, 16);
    assert_feed_painted(&mut engine, view, "throttled scroll fill");
}

/// `data-src` swapping (lazysizes style): IO swaps the source and the row's
/// caption shows.
#[test]
fn io_data_src_swap_rows() {
    let script = format!(
        "{FILL} var io = new IntersectionObserver(function (es) {{ es.forEach(function (e) {{ \
         if (!e.isIntersecting) return; var img = e.target.querySelector('img'); \
         img.src = img.getAttribute('data-src'); fill(e.target); io.unobserve(e.target); }}); }}); \
         document.querySelectorAll('.row').forEach(function (r) {{ var img = document.createElement('img'); \
           img.setAttribute('data-src', 'data:image/gif;base64,R0lGODlhAQABAIAAAP///wAAACH5BAEAAAAALAAAAAABAAEAAAICRAEAOw=='); \
           img.width = 10; img.height = 10; r.appendChild(img); io.observe(r); }});"
    );
    let (mut engine, view, _server) = load(&feed("", true, &script));
    flick(&mut engine, view);
    flick(&mut engine, view);
    pump(&mut engine, view, 200, 16);
    assert_feed_painted(&mut engine, view, "IO data-src swap");
}

