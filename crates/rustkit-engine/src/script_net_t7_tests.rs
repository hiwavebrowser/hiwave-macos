//! T7a: live script requests on the `rustkit-net` runtime (T7-design.md),
//! against a real loopback server and the real policy: no engine, no GPU.

use crate::inspector::{Inspector, InspectorConfig, InspectorEvent, InspectorRecord};
use crate::script_net::{start_live, LiveRequest, FAIL_NAMING_URL_PATH, MAX_LIVE_NET_TASKS, TASK_PANIC_PATH};
use crate::script_net_tests::serve_routes;
use rustkit_bindings::{NetDelivery, NetRequest};
use rustkit_net::policy::FetchPolicy;
use rustkit_net::{LoaderConfig, ResourceLoader};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use url::Url;

struct Rig {
    policy: Arc<FetchPolicy>,
    loader: Arc<ResourceLoader>,
    inspector: Arc<Inspector>,
    rt: tokio::runtime::Runtime,
    port: u16,
    hits: Arc<std::sync::atomic::AtomicUsize>,
}

fn rig() -> Rig {
    let server = serve_routes(Vec::new());
    Rig {
        policy: Arc::new(FetchPolicy::for_page(
            Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap(),
            None,
        )),
        loader: Arc::new(ResourceLoader::new(LoaderConfig::default()).unwrap()),
        inspector: Inspector::new(&InspectorConfig { buffer: true, capacity: 8192, ..Default::default() }),
        // The page's runtime: it only awaits the outcome.
        rt: tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap(),
        port: server.port,
        hits: server.hits,
    }
}

impl Rig {
    fn start(&self, id: u64, path: &str, out: usize) -> LiveRequest {
        let request = NetRequest {
            id,
            method: "GET".into(),
            url: format!("http://127.0.0.1:{}{path}", self.port),
            headers: Vec::new(),
            body_b64: None,
            mode: "cors".into(),
            credentials: "same-origin".into(),
            redirect: "follow".into(),
            destination: "fetch".into(),
        };
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        start_live(Arc::clone(&self.policy), Arc::clone(&self.loader), request, deadline, Arc::clone(&self.inspector), out)
    }

    fn records(&self) -> Vec<InspectorRecord> {
        self.inspector.snapshot_since(0).records
    }

    /// Request ids, and how many terminal records each has.
    fn terminals(&self) -> Vec<(u64, usize)> {
        let records = self.records();
        records
            .iter()
            .filter_map(|r| match r.event {
                InspectorEvent::Request { id, .. } => Some(id),
                _ => None,
            })
            .map(|id| {
                let n = records
                    .iter()
                    .filter(|r| {
                        matches!(r.event, InspectorEvent::Response { id: i, .. } | InspectorEvent::Failure { id: i, .. } if i == id)
                    })
                    .count();
                (id, n)
            })
            .collect()
    }

    fn wait_for(&self, budget: Duration, done: impl Fn(&Self) -> bool) -> bool {
        let deadline = Instant::now() + budget;
        while Instant::now() < deadline {
            if done(self) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        done(self)
    }
}

fn failures(records: &[InspectorRecord]) -> Vec<String> {
    records
        .iter()
        .filter_map(|r| match &r.event {
            InspectorEvent::Failure { reason, .. } => Some(reason.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_live_request_runs_and_is_recorded_without_the_page_polling_it() {
    // The page side is never polled until the record is there: the work and
    // the record happen on the network runtime.
    let rig = rig();
    let live = rig.start(1, "/hello", 0);
    assert!(
        rig.wait_for(Duration::from_secs(5), |rig| rig.terminals() == vec![(1, 1)]),
        "no response record while nothing polled the request: {:?}",
        rig.records()
    );
    let (id, outcome) = rig.rt.block_on(live);
    assert_eq!(id, 1);
    assert!(matches!(outcome, NetDelivery::Response { status: 200, .. }), "{outcome:?}");
}

#[test]
fn abort_against_completion_leaves_exactly_one_terminal_record_per_request() {
    // T7 test 4: dropping the page side races the task finishing. Either
    // way there is one response or one cancel, never both, never none.
    let rig = rig();
    let n = 60u64;
    for id in 1..=n {
        let live = rig.start(id, "/hello", 0);
        // Spread the drops over the request's life.
        std::thread::sleep(Duration::from_micros((id % 12) * 250));
        drop(live);
    }
    let all_answered = rig.wait_for(Duration::from_secs(10), |rig| {
        let t = rig.terminals();
        t.len() == n as usize && t.iter().all(|(_, k)| *k >= 1)
    });
    let terminals = rig.terminals();
    assert!(all_answered, "a request has no terminal record: {terminals:?}");
    assert!(terminals.iter().all(|(_, k)| *k == 1), "a request has two terminal records: {terminals:?}");
}

#[test]
fn past_the_views_task_cap_a_request_fails_at_once_and_nothing_is_sent() {
    // T7 test 5 (the spawned-task cap; the policy caps are T2's tests).
    let rig = rig();
    let live = rig.start(1, "/hello", MAX_LIVE_NET_TASKS);
    let (_, outcome) = rig.rt.block_on(live);
    assert!(matches!(outcome, NetDelivery::Error(_)), "{outcome:?}");
    assert_eq!(failures(&rig.records()), vec!["too many script requests".to_string()]);
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(rig.hits.load(Ordering::SeqCst), 0, "a request over the cap reached the server");
    // One under the cap still runs.
    let (_, outcome) = rig.rt.block_on(rig.start(2, "/hello", MAX_LIVE_NET_TASKS - 1));
    assert!(matches!(outcome, NetDelivery::Response { status: 200, .. }), "{outcome:?}");
}

#[test]
fn a_panicking_request_task_is_an_engine_failure_and_others_go_on() {
    // T7 test 10 (C3: injected where the policy runs, inside the unwind).
    let rig = rig();
    let panicking = rig.start(1, TASK_PANIC_PATH, 0);
    let fine = rig.start(2, "/hello", 1);
    let (_, outcome) = rig.rt.block_on(panicking);
    assert!(matches!(outcome, NetDelivery::Error(_)), "{outcome:?}");
    let (_, outcome) = rig.rt.block_on(fine);
    assert!(matches!(outcome, NetDelivery::Response { status: 200, .. }), "{outcome:?}");
    assert!(rig.wait_for(Duration::from_secs(5), |rig| rig.terminals().len() == 2 && rig.terminals().iter().all(|t| t.1 == 1)));
    let reasons = failures(&rig.records());
    assert_eq!(reasons, vec!["engine failure: request task panicked".to_string()], "not abandoned, exactly once");
}

#[test]
fn mirror_lines_from_the_network_runtime_carry_the_pages_nav_and_no_query() {
    // T7 tests 11 and 7 (mirror half): a denial is logged by the task, on a
    // rustkit-net worker; the test's sink on this thread still sees it, with
    // this thread's nav id, and without the query.
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            crate::console_log::force_for_test(None);
            crate::console_log::install_sink(None);
            rustkit_js::set_diag_nav(0);
        }
    }
    let _reset = Reset;
    let rig = rig();
    let lines = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    crate::console_log::force_for_test(Some(true));
    crate::console_log::install_sink(Some(Arc::clone(&lines)));
    rustkit_js::set_diag_nav(77);
    let request = NetRequest {
        id: 1,
        method: "GET".into(),
        // Another private address: the policy refuses it, in the task.
        url: "http://10.255.255.1/x?token=SECRET#frag".into(),
        headers: Vec::new(),
        body_b64: None,
        mode: "cors".into(),
        credentials: "same-origin".into(),
        redirect: "follow".into(),
        destination: "fetch".into(),
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let live = start_live(
        Arc::clone(&rig.policy),
        Arc::clone(&rig.loader),
        request,
        deadline,
        Arc::clone(&rig.inspector),
        0,
    );
    let (_, outcome) = rig.rt.block_on(live);
    assert!(matches!(outcome, NetDelivery::Error(_)), "{outcome:?}");
    let got = lines.lock().unwrap().clone();
    let net: Vec<_> = got.iter().filter(|l| l.starts_with("net ")).collect();
    assert!(!net.is_empty(), "no mirror line from the task: {got:?}");
    assert!(net.iter().all(|l| l.contains("nav=77")), "{net:?}");
    assert!(got.iter().all(|l| !l.contains("SECRET") && !l.contains("frag")), "{got:?}");
}

#[test]
fn the_kill_switch_and_an_unlistened_view_keep_requests_working() {
    // T7 test 9. HIWAVE_NET_RUNTIME=0: the page-polled path of before,
    // where nothing happens, not even the request record, until polled.
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            crate::script_net::force_page_polled_for_test(false);
        }
    }
    let _reset = Reset;
    let rig = rig();
    crate::script_net::force_page_polled_for_test(true);
    let live = rig.start(1, "/hello", 0);
    std::thread::sleep(Duration::from_millis(300));
    assert!(rig.records().is_empty(), "the page-polled path ran unpolled: {:?}", rig.records());
    assert_eq!(rig.hits.load(Ordering::SeqCst), 0);
    let (_, outcome) = rig.rt.block_on(live);
    assert!(matches!(outcome, NetDelivery::Response { status: 200, .. }), "{outcome:?}");
    assert_eq!(rig.terminals(), vec![(1, 1)]);
    crate::script_net::force_page_polled_for_test(false);

    // Ring off and no panel: no guard, no records, and the request runs.
    let quiet = Rig { inspector: Inspector::new(&InspectorConfig::default()), ..rig };
    let (_, outcome) = quiet.rt.block_on(quiet.start(2, "/hello", 0));
    assert!(matches!(outcome, NetDelivery::Response { status: 200, .. }), "{outcome:?}");
    assert!(quiet.records().is_empty(), "{:?}", quiet.records());
}

#[test]
fn the_kill_switch_is_off_only_for_its_three_words() {
    // T7 test 9, the env half: what `HIWAVE_NET_RUNTIME` turns off.
    use crate::script_net::net_runtime_switch;
    for off in ["0", "off", "false"] {
        assert!(!net_runtime_switch(Some(off)), "{off}");
    }
    for on in [None, Some(""), Some("1"), Some("on"), Some("OFF"), Some(" 0")] {
        assert!(net_runtime_switch(on), "{on:?}");
    }
}

#[test]
fn records_written_on_the_network_runtime_carry_no_query() {
    // T7 test 7, the records half (the mirror half is with the
    // HIWAVE_CONSOLE_LOG tests).
    let rig = rig();
    let (_, ok) = rig.rt.block_on(rig.start(1, "/hello?token=SECRET#frag", 0));
    assert!(matches!(ok, NetDelivery::Response { status: 200, .. }), "{ok:?}");
    let (_, failed) = rig.rt.block_on(rig.start(2, "/__t7_task_panic?token=SECRET", 0));
    assert!(matches!(failed, NetDelivery::Error(_)));
    // A failure reason that names the request's full URL, written on the
    // network runtime: the scrub at record time takes the query out of it.
    let (_, named) = rig.rt.block_on(rig.start(3, &format!("{FAIL_NAMING_URL_PATH}?token=SECRET#frag"), 0));
    assert!(matches!(named, NetDelivery::Error(_)));
    assert!(rig.wait_for(Duration::from_secs(5), |rig| rig.terminals().len() == 3 && rig.terminals().iter().all(|t| t.1 == 1)));
    let reasons = failures(&rig.records());
    assert!(
        reasons.iter().any(|r| r.contains(&format!("127.0.0.1:{}{FAIL_NAMING_URL_PATH}", rig.port))),
        "the URL-naming reason is not there: {reasons:?}"
    );
    let json = serde_json::to_string(&rig.records()).unwrap();
    assert!(!json.contains("SECRET") && !json.contains("frag"), "{json}");
}
