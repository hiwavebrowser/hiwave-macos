//! The Inspector's buffer, sinks, text parsing and summary, without an
//! engine.

use crate::inspector::*;

fn console(message: &str) -> InspectorEvent {
    InspectorEvent::Console {
        level: ConsoleLevel::Log,
        message: message.into(),
        truncated: false,
        location: None,
    }
}

fn buffered(capacity: usize) -> std::sync::Arc<Inspector> {
    Inspector::new(&InspectorConfig { buffer: true, capacity, keep_url_query: false })
}

#[test]
fn the_ring_buffer_drops_the_oldest_and_counts_them() {
    let inspector = buffered(3);
    for i in 0..5 {
        inspector.emit(console(&i.to_string()));
    }
    let snapshot = inspector.snapshot_since(0);
    let seqs: Vec<u64> = snapshot.records.iter().map(|r| r.seq).collect();
    assert_eq!(seqs, vec![2, 3, 4]);
    assert_eq!(snapshot.dropped, 2);
    assert_eq!(snapshot.next_seq, 5);
    // Since n: only what came at or after n.
    let later: Vec<u64> = inspector.snapshot_since(4).records.iter().map(|r| r.seq).collect();
    assert_eq!(later, vec![4]);
    assert!(inspector.snapshot_since(5).records.is_empty());
}

/// Formatting it is a failure: proves a path did not format.
struct MustNotFormat;

impl std::fmt::Display for MustNotFormat {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        panic!("formatted a string nobody asked for")
    }
}

/// Every recording path returns after one atomic load: no event is built,
/// no string formatted or copied, the lock is not taken.
#[test]
fn when_nothing_listens_no_event_is_built() {
    let inspector = Inspector::new(&InspectorConfig::default());
    assert!(!inspector.is_active());
    inspector.emit_with(|_| panic!("built an event nobody asked for"));
    inspector.console(ConsoleLevel::Log, "x", None);
    inspector.laid_out();
    inspector.painted();
    inspector.relayout(&MustNotFormat, 1.0);
    inspector.unsupported_api("A", "b", 1, None);
    inspector.alias_script("script#1", "https://a.test/s.js");
    let url = url::Url::parse("https://a.test/?q=1").unwrap();
    inspector.navigation_started(Some(&url));
    let snapshot = inspector.snapshot_since(0);
    assert!(snapshot.records.is_empty());
    assert_eq!(snapshot.next_seq, 0);
}

#[test]
fn turning_the_buffer_off_empties_it_and_stops_recording() {
    let inspector = buffered(10);
    inspector.emit(console("a"));
    inspector.set_buffer_enabled(false);
    assert!(!inspector.is_active());
    inspector.emit_with(|_| panic!("off"));
    assert!(inspector.snapshot_since(0).records.is_empty());
}

#[test]
fn the_inspector_can_be_shared_across_threads() {
    fn shared<T: Send + Sync>() {}
    shared::<Inspector>();
}

#[test]
fn records_serialize_with_a_stable_field_order() {
    let record = InspectorRecord {
        seq: 7,
        t_ms: 1.5,
        event: InspectorEvent::ScriptError {
            name: "TypeError".into(),
            message: "x".into(),
            location: Some(SourceLocation { url: "https://a.test/s.js".into(), line: 3, column: 9 }),
            stack: None,
            unhandled_rejection: false,
            script: "https://a.test/s.js".into(),
        },
    };
    assert_eq!(
        serde_json::to_string(&record).unwrap(),
        r#"{"seq":7,"t_ms":1.5,"type":"script_error","name":"TypeError","message":"x","location":{"url":"https://a.test/s.js","line":3,"column":9},"stack":null,"unhandled_rejection":false,"script":"https://a.test/s.js"}"#
    );
    let back: InspectorRecord = serde_json::from_str(&serde_json::to_string(&record).unwrap()).unwrap();
    assert_eq!(back, record);
}

#[test]
fn error_text_is_taken_apart_without_running_anything() {
    let (name, message, at, stack) = parse_error_text(
        "TypeError: cannot read (inline#2:5:3)\n    at f (inline#2:5:3)\n    at <main> (inline#2:7:1)",
    );
    assert_eq!(name, "TypeError");
    assert_eq!(message, "cannot read");
    assert_eq!(at, Some(("inline#2".to_string(), 5, 3)));
    assert_eq!(stack.as_deref(), Some("    at f (inline#2:5:3)\n    at <main> (inline#2:7:1)"));

    // A thrown string: no name.
    let (name, message, _, _) = parse_error_text("\"oops\"\n    at <main> (inline#1:1:7)");
    assert_eq!(name, "");
    assert_eq!(message, "\"oops\"");

    // A URL path with colons in it, and Boa's unnamed-script form.
    let (_, _, at, _) = parse_error_text("Error: e\n    at g (https://a.test:8080/x.js:12:4)");
    assert_eq!(at, Some(("https://a.test:8080/x.js".to_string(), 12, 4)));
    let (_, message, at, _) = parse_error_text("Error: e (unknown at :2:8)");
    assert_eq!(message, "e");
    assert_eq!(at, Some((String::new(), 2, 8)));

    // `String(error)` from a listener: just the head.
    assert_eq!(
        parse_error_text("ReferenceError: f is not defined"),
        ("ReferenceError".into(), "f is not defined".into(), None, None)
    );
}

#[test]
fn cap_cuts_on_a_character_boundary() {
    assert_eq!(cap("héllo", 2), ("hé".to_string(), true));
    assert_eq!(cap("hé", 2), ("hé".to_string(), false));
}

#[test]
fn the_rolling_file_sink_rotates_and_keeps_at_most_n_files() {
    let dir = std::env::temp_dir().join(format!("rustkit-inspector-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("events.jsonl");
    let mut sink = RollingFileSink::new(&path, 300, 2).unwrap();
    for i in 0..40 {
        sink.record(&InspectorRecord { seq: i, t_ms: 0.0, event: console("0123456789") });
    }
    sink.flush();
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, vec!["events.jsonl", "events.jsonl.1", "events.jsonl.2"]);
    for name in &names {
        let text = std::fs::read_to_string(dir.join(name)).unwrap();
        assert!(text.len() <= 300, "{name} is {} bytes", text.len());
        for line in text.lines() {
            serde_json::from_str::<InspectorRecord>(line).expect("whole lines only");
        }
    }
    // The newest record is in the live file.
    let live = std::fs::read_to_string(&path).unwrap();
    assert!(live.lines().last().unwrap().starts_with("{\"seq\":39,"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_summary_counts_by_kind_and_takes_final_unsupported_counts() {
    let records: Vec<InspectorRecord> = [
        console("a"),
        InspectorEvent::Console { level: ConsoleLevel::Error, message: "e".into(), truncated: false, location: None },
        InspectorEvent::ScriptError {
            name: "Error".into(),
            message: "m".into(),
            location: None,
            stack: None,
            unhandled_rejection: true,
            script: "inline#1".into(),
        },
        InspectorEvent::UnsupportedApi { interface: "XMLHttpRequest".into(), member: "open(async=false)".into(), location: None },
    ]
    .into_iter()
    .enumerate()
    .map(|(i, event)| InspectorRecord { seq: i as u64, t_ms: 0.0, event })
    .collect();
    let unsupported = vec![UnsupportedApiCount {
        interface: "XMLHttpRequest".into(),
        member: "open(async=false)".into(),
        count: 4,
        location: None,
    }];
    let summary = InspectorSummary::from_records(&records, &unsupported);
    assert_eq!(
        serde_json::to_string(&summary).unwrap(),
        r#"{"schema":2,"events":4,"console":{"log":1,"info":0,"warn":0,"error":1,"debug":0},"script_errors":1,"unhandled_rejections":1,"unsupported_apis":{"XMLHttpRequest.open(async=false)":4},"requests":{},"failures_by_stage":{}}"#
    );
}

#[test]
fn a_request_guard_answers_once_and_a_dropped_one_fails() {
    let inspector = buffered(16);
    let url = url::Url::parse("https://a.test/x.png?q=1").unwrap();
    let answered = inspector.begin_request(&url, "GET", ResourceType::Image, Some("parser")).unwrap();
    answered.response(200, Some("image/png; charset=binary"), Some(10), Some(10), false);
    let dropped = inspector
        .begin_request(&url, "GET", ResourceType::Image, None)
        .unwrap()
        .dropped_as(FailureStage::OverBudget, "subresource budget spent");
    drop(dropped);
    let failed = inspector.begin_request(&url, "GET", ResourceType::Fetch, Some("script")).unwrap();
    failed.fail(FailureStage::Cors, "CORS check failed for https://b.test/?secret=1");
    assert_eq!(inspector.open_requests(), 0);
    let events: Vec<InspectorEvent> = inspector.snapshot_since(0).records.into_iter().map(|r| r.event).collect();
    assert!(matches!(&events[0], InspectorEvent::Request { id: 1, url, .. } if url == "https://a.test/x.png"));
    assert!(matches!(&events[1], InspectorEvent::Response { id: 1, status: 200, mime: Some(m), .. } if m == "image/png"));
    assert!(matches!(&events[3], InspectorEvent::Failure { id: 2, stage: FailureStage::OverBudget, .. }));
    assert!(
        matches!(&events[5], InspectorEvent::Failure { id: 3, stage: FailureStage::Cors, reason, .. } if reason == "CORS check failed for https://b.test/")
    );
    // Off: no guard, nothing built.
    let off = Inspector::new(&InspectorConfig::default());
    assert!(off.begin_request(&url, "GET", ResourceType::Image, None).is_none());
}
