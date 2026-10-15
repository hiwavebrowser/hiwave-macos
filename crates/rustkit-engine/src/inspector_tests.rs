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
    Inspector::new(&InspectorConfig { buffer: true, capacity, ..Default::default() })
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
    inspector.relayout(Some(&crate::RelayoutCause::Timer), 1.0);
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
            message: None,
            location: Some(SourceLocation { url: "https://a.test/s.js".into(), line: 3, column: 9 }),
            stack: None,
            unhandled_rejection: false,
            script: "https://a.test/s.js".into(),
            value: "error".into(),
            message_len: Some(12),
        },
    };
    assert_eq!(
        serde_json::to_string(&record).unwrap(),
        r#"{"seq":7,"t_ms":1.5,"type":"script_error","name":"TypeError","message":null,"location":{"url":"https://a.test/s.js","line":3,"column":9},"stack":null,"unhandled_rejection":false,"script":"https://a.test/s.js","value":"error","message_len":12}"#
    );
    let back: InspectorRecord = serde_json::from_str(&serde_json::to_string(&record).unwrap()).unwrap();
    assert_eq!(back, record);
}

/// A frame's script is an engine label or a URL as the URL parser reads it
/// (any scheme, tabs and newlines dropped, backslashes as slashes), reduced
/// to origin and path; anything else is no place at all.
#[test]
fn a_frames_script_is_a_label_or_a_url_reduced_by_the_url_parser() {
    let inspector = Inspector::new(&InspectorConfig { buffer: true, ..InspectorConfig::default() });
    inspector.alias_script("script#2", "https://a.test/app.js?v=SECRET");
    let at = |path: &str| inspector.frame_location(path, 1, 2).map(|l| l.url);
    assert_eq!(at("inline#3").as_deref(), Some("inline#3"));
    assert_eq!(at("script#2").as_deref(), Some("https://a.test/app.js"));
    // An inline module is `inline#n` because the engine named it so (an
    // alias), never because a URL ends in `#inline-module-n`: a page can
    // import `./x.js#inline-module-1` (third R1 hold @ 6bda3b2c).
    inspector.alias_script("https://doc.test/#inline-module-4", "inline#4");
    assert_eq!(at("https://doc.test/#inline-module-4").as_deref(), Some("inline#4"));
    assert_eq!(inspector.url_text("https://doc.test/#inline-module-4"), "inline#4");
    assert_eq!(at("https://doc.test/x.js#inline-module-1").as_deref(), Some("https://doc.test/x.js"));
    assert_eq!(inspector.url_text("https://doc.test/x.js#inline-module-1"), "https://doc.test/x.js");
    assert_eq!(at("http://[::1]:9/x?token=S").as_deref(), Some("http://[::1]:9/x"));
    assert_eq!(at("ht\ttps://h.test/x?token=S").as_deref(), Some("https://h.test/x"));
    assert_eq!(at("https:\\h.test\\x?token=S").as_deref(), Some("https://h.test/x"));
    assert_eq!(at("ftp://u:PW@h.test/x?token=S#f").as_deref(), Some("ftp://h.test/x"));
    assert_eq!(at("data:text/javascript,SECRET").as_deref(), Some("data:text/javascript,…"));
    assert_eq!(at("javascript:SECRET").as_deref(), Some("javascript:"));
    for unreadable in ["inline#", "inline#x", "//h.test/x?token=S", "/api?token=S", "x y", ""] {
        assert_eq!(at(unreadable), None, "{unreadable:?}");
    }
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
            message: None,
            location: None,
            stack: None,
            unhandled_rejection: true,
            script: "inline#1".into(),
            value: "error".into(),
            message_len: Some(1),
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
        r#"{"schema":1,"events":4,"console":{"log":1,"info":0,"warn":0,"error":1,"debug":0},"script_errors":1,"unhandled_rejections":1,"unsupported_apis":{"XMLHttpRequest.open(async=false)":4}}"#
    );
}

/// A query runs past brackets, parens and quotes to whitespace; a frame's
/// `:line:col)` and closing punctuation after it are kept, and a frame
/// whose URL's query holds a `(` is still read as that URL.
#[test]
fn a_query_is_scrubbed_up_to_whitespace_or_a_frames_position() {
    let inspector = buffered(1);
    for (text, scrubbed) in [
        ("bad https://h.test/x?a[]=1&t=S", "bad https://h.test/x"),
        ("see (https://h.test/y?a=(1)&t=S) now", "see (https://h.test/y) now"),
        ("q 'https://h.test/z?a='1'&t=S'.", "q 'https://h.test/z'."),
        ("    at f (https://h.test/m.js?a=(1)&t=S:2:7)", "    at f (https://h.test/m.js:2:7)"),
        ("Error: x (https://h.test/m.js?a=(1)&t=S:2:7)", "Error: x (https://h.test/m.js:2:7)"),
        ("at https://h.test/m.js?a=(1)&t=S:2:7) end", "at https://h.test/m.js:2:7) end"),
    ] {
        assert_eq!(inspector.scrub_text(text), scrubbed, "{text}");
    }
    let keeping = Inspector::new(&InspectorConfig { buffer: true, keep_url_query: true, ..Default::default() });
    assert_eq!(
        keeping.scrub_text("bad https://u:p@h.test/x?a[]=1&t=S."),
        "bad https://h.test/x?a[]=1&t=S."
    );
}

/// A network error's stage comes from the error's kind, never its text: a
/// connection failure whose text says `lookup` (a host name, a server's
/// words) is still `connect` (third R1 hold @ 6bda3b2c, audit).
#[test]
fn a_network_errors_kind_is_never_read_from_its_text() {
    use rustkit_http::HttpError;
    use rustkit_net::NetError;
    for text in ["h2 handshake: lookup.test went away", "resolve this", "no addresses", "Name or service not known"] {
        let error = NetError::HttpError(HttpError::ConnectionFailed(text.into()));
        assert_eq!(net_error_kind(&error), "connect", "{text:?}");
    }
    let lookup = NetError::HttpError(HttpError::LookupFailed("nowhere.test: no addresses".into()));
    assert_eq!(net_error_kind(&lookup), "dns");
}
