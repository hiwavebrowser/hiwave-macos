//! The report page (`hiwave://report`) in the RustKit content view reads
//! its figures through the internal page's `window.ipc`
//! ([`crate::load_report_page`], [`crate::serve_internal_page_reads`]):
//! today's, this week's and this month's. A web page in the same view has
//! no such `window.ipc`, and nothing else the report page posts is run.

use super::*;
use rustkit_viewhost::Bounds;
use std::time::{Duration, Instant};
use webview_rustkit::RustKitView;

fn app_state(dir: &std::path::Path) -> Arc<Mutex<AppState>> {
    let config = crate::state::AppConfig {
        data_dir: dir.to_path_buf(),
        download_dir: dir.join("downloads"),
        ..Default::default()
    };
    Arc::new(Mutex::new(AppState::new(config).expect("app state")))
}

/// Three trackers blocked today.
fn with_three_trackers(state: &Arc<Mutex<AppState>>) {
    let s = state.lock().unwrap();
    for domain in ["a.example", "b.example", "c.example"] {
        s.analytics.track_tracker_blocked(domain, None).expect("track");
    }
}

fn trackers_today(state: &Arc<Mutex<AppState>>) -> u64 {
    let s = state.lock().unwrap();
    s.analytics.get_today_stats().expect("today").trackers_blocked as u64
}

fn content_view() -> RustKitView {
    RustKitView::headless_content(Bounds::new(0, 0, 1000, 800))
}

/// Turn the loop for `view` as the event loop does (load slices, the
/// page's clock, the report page's reads) until `done` or the deadline.
fn turn_until(view: &RustKitView, state: &Arc<Mutex<AppState>>, what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if !view.step_load() {
            view.process_events();
        }
        serve_internal_page_reads(view, state);
        if done() {
            return;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// One local web page, served at `/` for as long as the test runs.
fn web_page(body: &'static str) -> String {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            std::thread::spawn(move || {
                let mut request = Vec::new();
                let mut buf = [0u8; 2048];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    match stream.read(&mut buf) {
                        Ok(0) | Err(_) => return,
                        Ok(n) => request.extend_from_slice(&buf[..n]),
                    }
                }
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
            });
        }
    });
    format!("http://127.0.0.1:{port}/")
}

/// What a web page answers about the bridge: in Chrome every part is
/// `undefined` / `false`.
const BRIDGE_PROBE: &str = "[typeof window.ipc, 'ipc' in window, typeof window.hiwaveChrome, \
     typeof __rk_ipc_post, typeof __rustkit_app_read, typeof __rk].join(',')";
const NO_BRIDGE: &str = "undefined,false,undefined,undefined,undefined,undefined";

const LISTENING_PAGE: &str = "<title>web</title><p>web</p><script>window.__answers = 0; \
     window.addEventListener('hiwave-response', function () { window.__answers++; });</script>";

#[test]
fn the_report_page_shows_today_s_figures_read_through_the_bridge() {
    let dir = tempfile::tempdir().expect("temp dir");
    let state = app_state(dir.path());
    with_three_trackers(&state);
    let view = content_view();
    load_report_page(&view);
    turn_until(&view, &state, "the report page to show today's trackers", || {
        view.read_for_test("document.getElementById('stat-trackers').textContent").contains('3')
    });
    let shown = view.read_for_test(
        "document.getElementById('error-container').textContent + '|' + document.getElementById('loading').style.display",
    );
    assert!(shown.contains("|none"), "the page shows an error or is still loading: {shown}");
    assert!(!shown.contains("Failed"), "{shown}");

    // The other two reads the page offers.
    view.read_for_test("loadReport('weekly'); 0");
    turn_until(&view, &state, "the weekly read", || {
        view.read_for_test("document.getElementById('loading').style.display").contains("none")
    });
    view.read_for_test("loadReport('monthly'); 0");
    turn_until(&view, &state, "the monthly read", || {
        view.read_for_test("document.getElementById('loading').style.display").contains("none")
    });
    let shown = view.read_for_test("document.getElementById('error-container').textContent");
    assert!(!shown.contains("Failed"), "{shown}");
}

#[test]
fn a_web_page_in_the_content_view_has_no_bridge() {
    let dir = tempfile::tempdir().expect("temp dir");
    let state = app_state(dir.path());
    let view = content_view();
    view.wry_load_url(&web_page(LISTENING_PAGE)).unwrap();
    turn_until(&view, &state, "the web page", || view.read_for_test("document.title").contains("web"));
    let probe = view.read_for_test(BRIDGE_PROBE);
    assert!(probe.contains(NO_BRIDGE), "{probe}");
    // Whatever the page posts on a `window.ipc` of its own reaches nothing.
    view.read_for_test(
        "window.ipc = { postMessage: function () {} }; window.ipc.postMessage('{\"cmd\":\"get_today_stats\"}'); 0",
    );
    turn_until(&view, &state, "a few turns", {
        let mut n = 0;
        move || {
            n += 1;
            n > 20
        }
    });
    assert!(view.take_internal_reads().is_empty());
    // No answer the app sends reaches it, whatever navigation it names.
    for nav_id in 0..64 {
        view.answer_internal_read(nav_id, r#"{"type":"success","data":{}}"#.to_string());
    }
    let answers = view.read_for_test("String(window.__answers)");
    assert!(answers.contains('0') && !answers.contains('1'), "{answers}");
}

/// The report page asked, then the view went to a web page before the
/// answer came: the answer is dropped, not handed to the web page.
#[test]
fn an_answer_for_the_report_page_does_not_reach_the_web_page_that_replaced_it() {
    let dir = tempfile::tempdir().expect("temp dir");
    let state = app_state(dir.path());
    with_three_trackers(&state);
    let view = content_view();
    load_report_page(&view);
    let deadline = Instant::now() + Duration::from_secs(30);
    let (nav_id, body) = loop {
        if !view.step_load() {
            view.process_events();
        }
        if let Some(read) = view.take_internal_reads().into_iter().next() {
            break read;
        }
        assert!(Instant::now() < deadline, "the report page never asked");
        std::thread::sleep(Duration::from_millis(10));
    };
    view.wry_load_url(&web_page(LISTENING_PAGE)).unwrap();
    turn_until(&view, &state, "the web page", || view.read_for_test("document.title").contains("web"));
    view.answer_internal_read(nav_id, internal_page_read(&state, &body));
    let answers = view.read_for_test("String(window.__answers)");
    assert!(answers.contains('0') && !answers.contains('1'), "{answers}");
    let probe = view.read_for_test(BRIDGE_PROBE);
    assert!(probe.contains(NO_BRIDGE), "{probe}");
}

/// Only the three reads are served; reset, export and every other command
/// are answered with an error and not run.
#[test]
fn the_report_page_s_reset_and_export_are_not_served() {
    let dir = tempfile::tempdir().expect("temp dir");
    let state = app_state(dir.path());
    with_three_trackers(&state);
    for body in [
        r#"{"cmd":"reset_analytics_data"}"#,
        r#"{"cmd":"export_analytics_data","format":"json"}"#,
        r#"{"cmd":"get_custom_report","start_date":"2026-01-01","end_date":"2026-12-31"}"#,
        r#"{"cmd":"get_top_domains","limit":10}"#,
        r#"{"cmd":"create_tab","url":"https://example.com/"}"#,
        r#"{"cmd":"navigate","url":"https://example.com/"}"#,
        r#"{"cmd":"get_version"}"#,
        "not json",
    ] {
        let answer = internal_page_read(&state, body);
        assert!(answer.contains(r#""type":"error""#), "{body} -> {answer}");
    }
    assert_eq!(trackers_today(&state), 3, "the data is untouched");
    for body in [r#"{"cmd":"get_today_stats"}"#, r#"{"cmd":"get_weekly_report"}"#, r#"{"cmd":"get_monthly_report"}"#] {
        let answer = internal_page_read(&state, body);
        assert!(answer.contains(r#""type":"success""#), "{body} -> {answer}");
    }

    // The same through the page itself: its Reset posts, nothing resets.
    let view = content_view();
    load_report_page(&view);
    turn_until(&view, &state, "the report page", || {
        view.read_for_test("document.getElementById('stat-trackers').textContent").contains('3')
    });
    view.read_for_test(
        "window.__reset = 'pending'; sendIpc('reset_analytics_data', {}).then(function () { window.__reset = 'done'; }, \
         function (e) { window.__reset = 'refused'; }); 0",
    );
    turn_until(&view, &state, "the reset's answer", || !view.read_for_test("window.__reset").contains("pending"));
    assert!(view.read_for_test("window.__reset").contains("refused"));
    assert_eq!(trackers_today(&state), 3, "the data is untouched");
}
