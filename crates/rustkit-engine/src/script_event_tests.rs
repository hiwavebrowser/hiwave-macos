//! Every skipped or stopped page script leaves one `script_event`, with
//! every field the per-tab notice and the log line need, whatever stopped
//! it; a script that ran leaves none.

use super::error_page_script_tests::{serve, Server};
use super::*;

fn site(path: &str, _: &str) -> (u16, &'static str, Vec<(&'static str, &'static str)>, String) {
    let html = |body: &str| (200, "text/html", vec![], body.to_string());
    match path {
        "/size" => html(r#"<html><head><script src="/huge.js"></script><script>var after = 1;</script></head><body></body></html>"#),
        // Over the 64 MiB ceiling by a little.
        "/huge.js" => (
            200,
            "text/javascript",
            vec![],
            format!("var huge = true;\n{}", "// padding\n".repeat(MAX_PAGE_SCRIPT_BYTES / 11 + 100)),
        ),
        // A spin that reads the document stops at the deadline; one that
        // never calls the host is stopped by the watchdog.
        "/spin" => html("<html><head><script>while (true) { document.getElementById('x'); }</script></head><body></body></html>"),
        "/bare-spin" => html("<html><head><script>while (true) {}</script></head><body></body></html>"),
        "/missing" => html(r#"<html><head><script src="/gone.js"></script></head><body></body></html>"#),
        "/types" => html(
            r#"<html><head>
<script type="text/x-javascript">window.legacy = 1;</script>
<script type="application/ld+json">{"not": "a script"}</script>
<script type="text/template"><p>nor this</p></script>
<script>window.ran = 1;</script>
</head><body></body></html>"#,
        ),
        "/loop" => html("<html><head><script>for (;;) {}</script></head><body></body></html>"),
        "/clean" => html("<html><head><script>window.ok = 1;</script></head><body></body></html>"),
        _ => (404, "text/plain", vec![], "not found".into()),
    }
}

fn load(server: &Server, path: &str, config: EngineConfig) -> (Engine, EngineViewId) {
    let mut engine = Engine::new(config).expect("engine");
    let view = engine
        .create_headless_view(Bounds {
            x: 0,
            y: 0,
            width: 300,
            height: 200,
        })
        .expect("view");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(engine.load_url(view, server.url(path)))
        .expect("load");
    (engine, view)
}

fn interrupting() -> EngineConfig {
    EngineConfig {
        script_budget_ms: 200,
        interrupt_scripts_at_budget: true,
        // The budget stops the spin, not Boa's loop limit.
        script_loop_iteration_limit: u64::MAX / 2,
        ..EngineConfig::default()
    }
}

/// The one event of a load, with the fields every event has checked.
fn only_event(engine: &Engine, view: EngineViewId, server: &Server, path: &str) -> ScriptEvent {
    let events = engine.script_events(view).unwrap();
    assert_eq!(events.len(), 1, "{events:#?}");
    let event = events[0].clone();
    assert_eq!(event.document, server.url(path).as_str(), "{event:#?}");
    assert!(event.at_ms < 60_000, "{event:#?}");
    assert_eq!(
        (event.parse_ms, event.compile_ms, &event.initiator),
        (None, None, &None)
    );
    event
}

#[test]
fn a_script_over_the_ceiling_is_skipped_for_its_size() {
    let server = serve(site);
    let (engine, view) = load(&server, "/size", EngineConfig::default());
    let event = only_event(&engine, view, &server, "/size");
    assert_eq!(event.reason, ScriptEventReason::SkippedSize, "{event:#?}");
    assert_eq!(event.source, server.url("/huge.js").as_str());
    assert_eq!(event.kind, ScriptKind::Classic);
    assert!(event.bytes > MAX_PAGE_SCRIPT_BYTES);
    assert_eq!(event.limit, Some("max_page_script_bytes"));
    assert_eq!(event.limit_value, Some(64 * 1024 * 1024));
    assert_eq!(event.js_ms, 0);
    let head = event.head.as_deref().unwrap();
    assert!(head.starts_with("var huge = true;\n// padding"), "{head}");
    assert_eq!(head.chars().count(), 200);
    assert_eq!(event.sha256, None);
    assert_eq!((event.http_status, &event.detail), (None, &None));
}

#[test]
fn a_spin_that_calls_the_host_is_stopped_at_the_budget() {
    let server = serve(site);
    let (engine, view) = load(&server, "/spin", interrupting());
    let event = only_event(&engine, view, &server, "/spin");
    assert_eq!(event.reason, ScriptEventReason::StoppedBudget, "{event:#?}");
    assert_eq!(event.source, "inline#1");
    assert_eq!(event.kind, ScriptKind::Inline);
    assert_eq!(
        event.bytes,
        "while (true) { document.getElementById('x'); }".len()
    );
    assert_eq!(event.limit, Some("script_budget_ms"));
    assert_eq!(event.limit_value, Some(200));
    assert!(event.js_ms >= 150, "{event:#?}");
    // Inline: a fingerprint, not a quote.
    assert_eq!(event.head, None);
    let sha = event.sha256.as_deref().unwrap();
    assert_eq!(sha.len(), 64);
    assert!(sha.chars().all(|c| c.is_ascii_hexdigit()));
    // The stop held the window for as long as the script ran.
    let notice = engine.script_notice(view).unwrap();
    assert_eq!(notice.count, 1);
    assert_eq!(notice.longest_hold_ms, event.js_ms);
    assert_eq!(notice.longest_hold_source.as_deref(), Some("inline#1"));
}

#[test]
fn a_spin_that_never_calls_the_host_is_stopped_by_the_watchdog() {
    let server = serve(site);
    let (engine, view) = load(&server, "/bare-spin", interrupting());
    let event = only_event(&engine, view, &server, "/bare-spin");
    assert_eq!(
        event.reason,
        ScriptEventReason::StoppedWatchdog,
        "{event:#?}"
    );
    assert_eq!(
        (event.limit, event.limit_value),
        (Some("script_budget_ms"), Some(200))
    );
    assert!(event.js_ms >= 150, "{event:#?}");
}

#[test]
fn a_script_that_is_not_found_is_a_fetch_error_with_its_status() {
    let server = serve(site);
    let (engine, view) = load(&server, "/missing", EngineConfig::default());
    let event = only_event(&engine, view, &server, "/missing");
    assert_eq!(event.reason, ScriptEventReason::FetchError, "{event:#?}");
    assert_eq!(event.source, server.url("/gone.js").as_str());
    assert_eq!(event.kind, ScriptKind::Classic);
    assert_eq!(event.http_status, Some(404));
    assert_eq!((event.bytes, event.js_ms), (0, 0));
    assert_eq!((event.limit, event.limit_value), (None, None));
    assert_eq!((&event.head, &event.sha256), (&None, &None));
}

#[test]
fn a_type_chromium_runs_and_the_engine_does_not_is_unsupported() {
    let server = serve(site);
    let (mut engine, view) = load(&server, "/types", EngineConfig::default());
    // Data blocks (`ld+json`, templates) are not scripts: no event.
    let event = only_event(&engine, view, &server, "/types");
    assert_eq!(
        event.reason,
        ScriptEventReason::UnsupportedType,
        "{event:#?}"
    );
    assert_eq!(event.source, "inline#unsupported-1");
    assert_eq!(event.kind, ScriptKind::Inline);
    // Its source as the page reads it.
    let length = engine
        .execute_script(view, "document.scripts[0].text.length")
        .unwrap();
    assert_eq!(length, format!("Number({}.0)", event.bytes));
    assert_eq!(event.detail.as_deref(), Some("type=text/x-javascript"));
    assert_eq!(event.sha256.as_deref().map(str::len), Some(64));
    assert_eq!((event.js_ms, event.limit), (0, None));
    // The page's other script keeps its position and runs.
    assert!(engine
        .script_log(view)
        .unwrap()
        .iter()
        .any(|r| r.source == "inline#1" && r.outcome == ScriptOutcome::Ran));
}

#[test]
fn boas_loop_limit_is_an_iteration_limit_event() {
    let server = serve(site);
    let config = EngineConfig {
        script_loop_iteration_limit: 10_000,
        ..EngineConfig::default()
    };
    let (engine, view) = load(&server, "/loop", config);
    let event = only_event(&engine, view, &server, "/loop");
    assert_eq!(
        event.reason,
        ScriptEventReason::IterationLimit,
        "{event:#?}"
    );
    assert_eq!(
        (event.limit, event.limit_value),
        (Some("script_loop_iteration_limit"), Some(10_000))
    );
    assert!(event.detail.is_some());
}

#[test]
fn a_page_whose_scripts_all_ran_has_no_events_and_a_new_document_starts_clean() {
    let server = serve(site);
    let (mut engine, view) = load(&server, "/missing", EngineConfig::default());
    assert_eq!(engine.script_notice(view).unwrap().count, 1);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(engine.load_url(view, server.url("/clean")))
        .expect("load");
    let notice = engine.script_notice(view).unwrap();
    assert_eq!((notice.count, notice.events.len()), (0, 0), "{notice:#?}");
    assert_eq!(notice.document, server.url("/clean").as_str());
}

/// The payload the browser's chrome reads (`window.hiwaveScriptNotice`):
/// its field names and spellings are the chrome's contract.
#[test]
fn the_notice_serializes_as_the_chrome_reads_it() {
    let server = serve(site);
    let (engine, view) = load(&server, "/missing", EngineConfig::default());
    let notice = serde_json::to_value(engine.script_notice(view).unwrap()).unwrap();
    let keys: Vec<&str> = notice
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    for key in [
        "generation",
        "document",
        "count",
        "longest_hold_ms",
        "longest_hold_source",
        "events",
    ] {
        assert!(keys.contains(&key), "{key} missing from {notice}");
    }
    assert_eq!(notice["count"], 1);
    let event = &notice["events"][0];
    for key in [
        "source",
        "document",
        "kind",
        "bytes",
        "reason",
        "http_status",
        "detail",
        "limit",
        "limit_value",
        "js_ms",
        "parse_ms",
        "compile_ms",
        "initiator",
        "at_ms",
        "head",
        "sha256",
    ] {
        assert!(event.get(key).is_some(), "{key} missing from {event}");
    }
    assert_eq!(event["reason"], "fetch-error");
    assert_eq!(event["kind"], "classic");
    assert_eq!(event["http_status"], 404);
    assert_eq!(serde_json::to_value(ScriptKind::Data).unwrap(), "data:");
    assert_eq!(
        serde_json::to_value(ScriptEventReason::StoppedWatchdog).unwrap(),
        "stopped-watchdog"
    );
}
