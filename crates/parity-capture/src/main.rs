//! Headless frame capture tool for parity testing.
//!
//! This tool renders HTML files using RustKit's headless mode and exports:
//! - PPM frame capture
//! - Layout tree JSON
//! - Performance metrics
//!
//! Unlike hiwave-smoke, this does NOT require a display and can run in CI.

mod inspector;
mod report;

use clap::Parser;
use rustkit_engine::inspector::{CollectingSink, JsonLinesSink};
use rustkit_engine::{EngineBuilder, EngineConfig, Inspector, InspectorSink, InspectorSummary, ScriptOutcome, ScriptRecord};
use std::sync::Arc;
use rustkit_viewhost::Bounds;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};
use tracing::{error, warn};
use url::Url;

#[derive(Parser, Debug, Clone)]
#[command(name = "parity-capture")]
#[command(about = "Headless frame capture for parity testing")]
#[command(group(clap::ArgGroup::new("source").required(true).args(["html_file", "url", "inspector_dump"])))]
struct Args {
    /// Path to HTML file to render
    #[arg(long)]
    html_file: Option<String>,

    /// Live URL to load through `Engine::load_url` (document + subresources
    /// over the network), for the real-site board
    #[arg(long)]
    url: Option<String>,

    /// Hard wall-clock limit for the whole capture, in milliseconds. On
    /// expiry the process prints a `timeout` result and exits 3.
    #[arg(long, default_value = "30000")]
    timeout_ms: u64,

    /// Output path for display-list JSON (paint commands, including text runs)
    #[arg(long)]
    dump_display_list: Option<String>,

    /// Output path for the page's script log (URL mode: one record per
    /// `<script>`, plus exceptions from lifecycle listeners and timers)
    #[arg(long)]
    dump_scripts: Option<String>,

    /// Viewport width
    #[arg(long, default_value = "1280")]
    width: u32,

    /// Viewport height
    #[arg(long, default_value = "800")]
    height: u32,

    /// After loading at --width x --height, resize the view to `WxH` (through
    /// `Engine::resize_view`, as a window resize does) and capture at the new
    /// size. Compare against a fresh load at `WxH` to find layout that stays
    /// stale across a resize.
    #[arg(long, value_parser = parse_size)]
    resize_to: Option<(u32, u32)>,

    /// Output path for PPM frame
    #[arg(long)]
    dump_frame: Option<String>,

    /// Output path for layout JSON
    #[arg(long)]
    dump_layout: Option<String>,

    /// JSON string or path to JSON file defining interaction sequence (wait/click/key/resize/capture)
    #[arg(long)]
    actions: Option<String>,

    /// Directory for action capture frames
    #[arg(long)]
    actions_out_dir: Option<String>,

    /// Optional HTTP replay proxy URL (e.g. http://127.0.0.1:8989) for deterministic HAR replay.
    /// Test-only; routes outbound requests to the local replay server while preserving document origin.
    #[arg(long)]
    replay_proxy: Option<String>,

    /// Virtual timer clock horizon in milliseconds (default: 5000).
    #[arg(long)]
    timer_horizon_ms: Option<u64>,

    /// Wall-clock budget for the page's scripts in milliseconds (default: 5000, the
    /// engine's and the board's). The live app runs pages at 60000; a capture taken
    /// with another budget is not comparable with the board and must be labelled.
    #[arg(long)]
    script_budget_ms: Option<u64>,

    /// Stop a script that is still running when the script budget is spent, as the
    /// live app does (off by default: the board lets a running script finish). A
    /// capture taken with it is not comparable with the board and must be labelled.
    #[arg(long)]
    interrupt_scripts: bool,

    /// After a URL load, turn the live loop as the app does for this many
    /// milliseconds of real time (timers, script requests, relayouts) and
    /// report what the turns did as `live_stats`. Off by default; a frame
    /// taken with it is not comparable with the board.
    #[arg(long)]
    live_ms: Option<u64>,

    /// Write the view's Inspector event log for the whole capture to this
    /// file, as JSON Lines (one record per line; see
    /// docs/INSPECTOR_EVENT_LOG.md). URLs lose their query and fragment.
    #[arg(long)]
    event_log: Option<String>,

    /// Add `event_summary` to the result: counts over the Inspector events
    /// (console calls by level, script errors, unsupported APIs by member,
    /// requests by type and outcome, failures by stage).
    #[arg(long)]
    event_summary: bool,

    /// Keep query strings and fragments in the event log's URLs. They can
    /// carry session ids and tokens: never for a log that will be shared.
    #[arg(long)]
    event_log_keep_query: bool,

    /// Keep the text of console messages in the event log (by default a
    /// message is its arguments' types and string lengths). Pages log
    /// cookies and form values: never for a log that will be shared.
    #[arg(long)]
    event_log_console_text: bool,

    /// Keep the message text of the page's errors in the event log (by
    /// default an error is its type, a standard name and its message's
    /// length). Pages throw cookies and form values: never for a log that
    /// will be shared.
    #[arg(long)]
    event_log_error_text: bool,

    /// Print the element diagnostics record (JSON) of the first element
    /// matching this CSS selector. Repeatable.
    #[arg(long, value_name = "SELECTOR")]
    diagnose: Vec<String>,

    /// Print the diagnostics record of the element hit at viewport point
    /// `X,Y` (CSS px). Repeatable.
    #[arg(long, value_name = "X,Y", value_parser = parse_point_arg)]
    diagnose_at: Vec<(f32, f32)>,

    /// Write a diagnostics record for every element (with or without a
    /// layout box) to this file.
    #[arg(long)]
    dump_diagnostics: Option<String>,

    /// Cap on the number of records `--dump-diagnostics` writes; the dump
    /// says `truncated: true` when the page had more elements.
    #[arg(long, default_value_t = rustkit_engine::DEFAULT_MAX_ELEMENTS)]
    diagnostics_max_elements: usize,

    /// Keep the page URL's query and fragment in diagnostics records
    /// (stripped by default).
    #[arg(long)]
    diagnostics_keep_query: bool,

    /// Write the page report (JSON): page state, covering boxes, content
    /// per viewport band and a ranked "most likely cause" list, from the
    /// final layout (dev-tools T4).
    #[arg(long, value_name = "FILE")]
    report: Option<String>,

    /// Write the page report as one page of text.
    #[arg(long, value_name = "FILE")]
    report_text: Option<String>,

    /// Turn script on for an --html-file load, so the load runs the page's
    /// inline classic scripts (fixture mode otherwise runs none), as for a
    /// built-in page a host loads directly. A script that throws fails the
    /// capture.
    #[arg(long)]
    run_inline_scripts: bool,

    /// Render the HiWave Inspector for this capture's diagnostics dump into
    /// this directory: inspector.html (the page with its input),
    /// inspector.png and inspector.display-list.json.
    #[arg(long, value_name = "DIR")]
    inspector_out: Option<String>,

    /// Show this dump (as --dump-diagnostics writes it) in the Inspector
    /// instead of capturing a page. Needs --inspector-out.
    #[arg(long, value_name = "FILE", requires = "inspector_out", conflicts_with_all = ["html_file", "url"])]
    inspector_dump: Option<String>,

    /// An inspector_diff.py result (tools/inspector) for the Inspector to
    /// mark and compare.
    #[arg(long, value_name = "FILE", requires = "inspector_out")]
    inspector_diff: Option<String>,

    /// Select the captured page's element at viewport point `X,Y` (CSS px)
    /// in the Inspector, as click-to-inspect will.
    #[arg(long, value_name = "X,Y", value_parser = parse_point_arg, requires = "inspector_out", conflicts_with = "inspector_dump")]
    inspector_select_at: Option<(f32, f32)>,

    /// Inspector viewport, `WxH`.
    #[arg(long, value_name = "WxH", value_parser = parse_size, default_value = "1280x800")]
    inspector_size: (u32, u32),

    /// Enable verbose output
    #[arg(long, short)]
    verbose: bool,
}

#[derive(Serialize, Deserialize)]
struct CaptureResult {
    status: String,
    html_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    url: Option<String>,
    width: u32,
    height: u32,
    frame_path: Option<String>,
    layout_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    display_list_path: Option<String>,
    layout_stats: Option<LayoutStats>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    script_stats: Option<ScriptStats>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    live_stats: Option<LiveStats>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    elapsed_ms: Option<u64>,
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    captures: Option<Vec<StepCapture>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    action_results: Option<Vec<ActionResult>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    event_log_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    event_summary: Option<InspectorSummary>,
    /// `--diagnose` / `--diagnose-at` records, in argument order; `null`
    /// where nothing matched.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    diagnostics: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    diagnostics_path: Option<String>,
    /// `--inspector-out`
    #[serde(skip_serializing_if = "Option::is_none", default)]
    inspector: Option<inspector::InspectorOutput>,
    /// `--report`: the layout half, built while the view exists; the event
    /// rules are added once the load's records are in (`run_capture`).
    #[serde(skip)]
    page_report: Option<report::Report>,
    /// True when this capture ran on the content thread (`HIWAVE_CONTENT_THREAD`).
    /// Absent means an older capture tool. `false` is the flag-off worker.
    #[serde(default)]
    content_thread: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ActionItem {
    #[serde(default)]
    step: Option<usize>,
    #[serde(rename = "type")]
    action_type: String,
    #[serde(default)]
    ms: Option<u64>,
    #[serde(default)]
    selector: Option<String>,
    #[serde(default)]
    x: Option<f32>,
    #[serde(default)]
    y: Option<f32>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    width: Option<u32>,
    #[serde(default)]
    height: Option<u32>,
    #[serde(default)]
    frame: Option<String>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    timeout_ms: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ActionsPayload {
    #[serde(default)]
    actions: Vec<ActionItem>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct StepCapture {
    step: usize,
    label: String,
    frame: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ActionResult {
    step: usize,
    #[serde(rename = "type")]
    action_type: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    frame: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    selector_used: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    method: Option<String>,
    elapsed_ms: u64,
}

impl CaptureResult {
    fn new(args: &Args) -> Self {
        CaptureResult {
            status: "ok".to_string(),
            html_file: args.html_file.clone(),
            url: args.url.clone(),
            width: args.width,
            height: args.height,
            frame_path: None,
            layout_path: None,
            display_list_path: None,
            layout_stats: None,
            script_stats: None,
            live_stats: None,
            elapsed_ms: None,
            error: None,
            captures: None,
            action_results: None,
            event_log_path: None,
            event_summary: None,
            diagnostics: None,
            diagnostics_path: None,
            inspector: None,
            page_report: None,
            content_thread: false,
        }
    }

    fn failed(mut self, status: &str, error: String) -> Self {
        self.status = status.to_string();
        self.error = Some(error);
        self
    }
}

/// Totals over a URL capture's script log.
#[derive(Serialize, Deserialize)]
struct ScriptStats {
    ran: u32,
    threw: u32,
    skipped: u32,
    fetch_failed: u32,
    over_budget: u32,
    bytes: u64,
    elapsed_ms: u64,
}

/// What the live loop did after the load (`--live-ms`).
#[derive(Serialize, Deserialize, Default)]
struct LiveStats {
    /// Real time the loop was given.
    wall_ms: u64,
    /// Of it, time spent inside turns: the window thread's time in the app.
    busy_ms: u64,
    turns: u32,
    timer_callbacks: u64,
    requests: u64,
    relayouts: u32,
    /// The page had no timer set and nothing in flight when the loop ended.
    idle_at_end: bool,
}

/// Wakes the live loop's sleep when work arrives off the page thread, as the
/// app's event loop is woken (`rustkit_engine::set_live_wake`): an inserted
/// script's fetch finishing is not `in_flight`, so without it a page with no
/// timer set slept the rest of `--live-ms` and its inserted scripts ran only
/// on the last turn.
#[derive(Default)]
struct LiveWake {
    woken: std::sync::Mutex<bool>,
    arrived: std::sync::Condvar,
}

impl LiveWake {
    /// The process's one waker, installed in the engine on first use.
    fn installed() -> Arc<LiveWake> {
        static WAKE: std::sync::OnceLock<Arc<LiveWake>> = std::sync::OnceLock::new();
        WAKE.get_or_init(|| {
            let wake = Arc::new(LiveWake::default());
            let engine_side = Arc::clone(&wake);
            rustkit_engine::set_live_wake(Arc::new(move || engine_side.wake()));
            wake
        })
        .clone()
    }

    fn wake(&self) {
        *self.woken.lock().unwrap_or_else(|e| e.into_inner()) = true;
        self.arrived.notify_one();
    }

    /// Sleep up to `limit`, less if woken (or woken since the last sleep).
    fn sleep(&self, limit: Duration) {
        let woken = self.woken.lock().unwrap_or_else(|e| e.into_inner());
        let (mut woken, _) = self
            .arrived
            .wait_timeout_while(woken, limit, |woken| !*woken)
            .unwrap_or_else(|e| e.into_inner());
        *woken = false;
    }
}

/// The app's pacing (`hiwave-app` `process_events`): a turn when the next
/// timer is due, a request is out or the engine wakes the loop. Timer and
/// request waits are never shorter than the last turn took (nor
/// `MIN_LIVE_TURN`). The engine's wake skips that floor and ends the wait at
/// once, as the app's poke does, so a page that keeps waking the loop gets
/// back-to-back turns.
fn run_live(
    rt: &tokio::runtime::Runtime,
    engine: &mut rustkit_engine::Engine,
    view_id: rustkit_engine::EngineViewId,
    live_ms: u64,
) -> LiveStats {
    const MIN_LIVE_TURN: Duration = Duration::from_millis(4);
    const LIVE_REQUEST_POLL: Duration = Duration::from_millis(10);
    let limit = Duration::from_millis(live_ms);
    let began = Instant::now();
    let mut clock = began;
    let mut stats = LiveStats::default();
    let wake = LiveWake::installed();
    tracing::info!(live_ms, "Live loop started");
    loop {
        let started = Instant::now();
        let elapsed_ms = started.duration_since(clock).as_millis() as u64;
        clock += Duration::from_millis(elapsed_ms);
        let turn = rt.block_on(engine.pump_live(view_id, elapsed_ms));
        stats.turns += 1;
        stats.timer_callbacks += turn.timers_ran as u64;
        stats.requests += turn.requests as u64;
        stats.relayouts += turn.relaid_out as u32;
        stats.busy_ms += started.elapsed().as_millis() as u64;
        let timer = turn.next_timer_ms.map(Duration::from_millis);
        let next = if turn.in_flight > 0 {
            Some(timer.map_or(LIVE_REQUEST_POLL, |t| t.min(LIVE_REQUEST_POLL)))
        } else {
            timer
        };
        stats.idle_at_end = next.is_none();
        let left = limit.saturating_sub(began.elapsed());
        if left.is_zero() {
            break;
        }
        // An idle page still gets its time: in the app only input, or the
        // engine's wake, ends the wait.
        let wait = next.map_or(left, |wait| wait.max(started.elapsed()).max(MIN_LIVE_TURN));
        wake.sleep(wait.min(left));
    }
    stats.wall_ms = began.elapsed().as_millis() as u64;
    stats
}

#[derive(Serialize, Deserialize)]
struct LayoutStats {
    total_boxes: u32,
    sized: u32,
    zero_size: u32,
    positioned: u32,
    at_origin: u32,
    sizing_rate: f32,
    positioning_rate: f32,
}

fn main() {
    let args = Args::parse();

    // Initialize tracing - respect RUST_LOG if set, otherwise use defaults
    let default_filter = if args.verbose { "info" } else { "warn" };
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| default_filter.to_string());
    tracing_subscriber::fmt()
        .with_env_filter(&filter)
        .with_writer(std::io::stderr)
        .init();

    // Hard wall-clock limit. A live page can stall the engine anywhere
    // (network, layout, paint) and nothing inside the engine can be trusted
    // to give up, so the limit is enforced from outside the capture thread.
    {
        let limit = Duration::from_millis(args.timeout_ms);
        let timeout_result = CaptureResult::new(&args).failed(
            "timeout",
            format!("capture exceeded {} ms", args.timeout_ms),
        );
        std::thread::spawn(move || {
            std::thread::sleep(limit);
            println!("{}", serde_json::to_string(&timeout_result).unwrap());
            std::process::exit(3);
        });
    }

    let started = Instant::now();
    let args_clone = args.clone();
    // Flag off: today's worker. Flag on: the page thread's name and stack,
    // so a 26-case run with HIWAVE_CONTENT_THREAD set is not the flag-off
    // capture again. The tool has no window, so the headless presenter stays
    // with the page on that thread.
    let (worker_name, worker_stack) = capture_worker_spec(rustkit_engine::content_thread_enabled());
    let handler = std::thread::Builder::new()
        .name(worker_name.into())
        .stack_size(worker_stack)
        .spawn(move || run_capture(&args_clone))
        .expect("failed to spawn capture worker thread");
    let mut result = match handler.join() {
        Ok(r) => r,
        Err(_) => CaptureResult::new(&args).failed("error", "worker thread panicked".to_string()),
    };
    result.elapsed_ms = Some(started.elapsed().as_millis() as u64);

    // Output JSON result
    println!("{}", serde_json::to_string(&result).unwrap());

    // Exit with appropriate code
    if result.status == "ok" {
        std::process::exit(0);
    } else {
        std::process::exit(1);
    }
}

/// Where a capture runs. `HIWAVE_CONTENT_THREAD` selects the content
/// thread (the name the page thread uses, and its stack). Anything else
/// keeps the capture worker the campaign has always used.
fn capture_worker_spec(content_thread: bool) -> (&'static str, usize) {
    if content_thread {
        ("page", rustkit_engine::content_stack_bytes())
    } else {
        ("capture-worker", 16 * 1024 * 1024)
    }
}

/// The user agent the shipping RustKit content view sends
/// (hiwave-app/src/webview_rustkit.rs). Live sites branch on it, so a URL
/// capture must present as the product does, not as a test tool.
const PRODUCT_USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.4 Safari/605.1.15 HiWave/1.0";

/// The capture, then its Inspector event log and summary, whichever way
/// the capture ended.
fn run_capture(args: &Args) -> CaptureResult {
    let wants_report = args.report.is_some() || args.report_text.is_some();
    let wants_events = args.event_log.is_some() || args.event_summary || wants_report;
    let sink = wants_events.then(CollectingSink::default);
    let mut inspector: Option<Arc<Inspector>> = None;
    let mut result = capture(args, sink.as_ref(), &mut inspector);
    result.content_thread = std::thread::current().name() == Some("page");
    if let Some(sink) = sink {
        let records = sink.records();
        if let Some(mut page) = result.page_report.take() {
            let json: Vec<serde_json::Value> =
                records.iter().filter_map(|r| serde_json::to_value(r).ok()).collect();
            report::apply_events(&mut page, &json);
            write_report_files(args, &page);
        }
        if let Some(ref path) = args.event_log {
            match write_event_log(path, &records) {
                Ok(()) => result.event_log_path = Some(path.clone()),
                Err(e) => error!("Failed to write event log: {:?}", e),
            }
        }
        if args.event_summary {
            let unsupported = inspector.map(|i| i.unsupported_apis()).unwrap_or_default();
            result.event_summary = Some(InspectorSummary::from_records(&records, &unsupported));
        }
    }
    result
}

fn write_event_log(path: &str, records: &[rustkit_engine::InspectorRecord]) -> std::io::Result<()> {
    let file = std::io::BufWriter::new(fs::File::create(path)?);
    let mut sink = JsonLinesSink::new(file);
    for record in records {
        sink.record(record);
    }
    sink.flush();
    Ok(())
}

fn capture(args: &Args, sink: Option<&CollectingSink>, inspector: &mut Option<Arc<Inspector>>) -> CaptureResult {
    let mut result = CaptureResult::new(args);

    if let Some(ref dump_path) = args.inspector_dump {
        let dump = match fs::read_to_string(dump_path) {
            Ok(d) => d,
            Err(e) => return result.failed("error", format!("Failed to read {dump_path}: {e}")),
        };
        if let Err(e) = write_inspector(args, &dump, None, &mut result) {
            return result.failed("error", e);
        }
        return result;
    }

    // Read HTML file (URL mode fetches inside the engine instead)
    let html_content = match &args.html_file {
        Some(html_file) => match fs::read_to_string(html_file) {
            Ok(content) => Some(preprocess_html(&content, Path::new(html_file))),
            Err(e) => {
                return result.failed("error", format!("Failed to read HTML file: {}", e));
            }
        },
        None => None,
    };
    let url = match &args.url {
        Some(raw) => match Url::parse(raw) {
            Ok(u) => Some(u),
            Err(e) => return result.failed("error", format!("Invalid URL: {}", e)),
        },
        None => None,
    };

    // Create engine with parity testing config (animations disabled).
    // Fixture mode keeps its historical test-tool UA; URL mode sends the
    // product's. URL mode runs the page's scripts, as the browser does;
    // fixture mode does not (the campaign fixtures are static pages).
    let user_agent = if url.is_some() {
        PRODUCT_USER_AGENT
    } else {
        "ParityCapture/1.0"
    };

    let replay_proxy = match &args.replay_proxy {
        Some(raw) => match Url::parse(raw) {
            Ok(u) => Some(u),
            Err(e) => return result.failed("error", format!("Invalid replay proxy URL: {}", e)),
        },
        None => None,
    };

    let engine_result = EngineBuilder::new()
        .with_config(capture_config(args, replay_proxy))
        .user_agent(user_agent)
        .javascript_enabled(url.is_some() || args.actions.is_some() || args.run_inline_scripts)
        .build();

    let mut engine = match engine_result {
        Ok(e) => e,
        Err(e) => return result.failed("error", format!("Failed to create engine: {:?}", e)),
    };

    // Create headless view
    let bounds = Bounds {
        x: 0,
        y: 0,
        width: args.width,
        height: args.height,
    };

    let view_id = match engine.create_headless_view(bounds) {
        Ok(id) => id,
        Err(e) => {
            return result.failed("error", format!("Failed to create headless view: {:?}", e));
        }
    };
    // Before the load: the whole load goes to the sink, none of it is
    // dropped by the view's ring buffer.
    if let Some(sink) = sink {
        let _ = engine.add_inspector_sink(view_id, Box::new(sink.clone()));
        *inspector = engine.inspector(view_id);
    }

    // Load: either the fixture HTML, or the live URL through the same
    // navigation path the browser uses (document fetch, then stylesheets,
    // fonts and images via load_subresources).
    if let Some(url) = url {
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => return result.failed("error", format!("Failed to create runtime: {}", e)),
        };
        if let Err(e) = rt.block_on(engine.load_url(view_id, url)) {
            return result.failed("error", format!("Failed to load URL: {:?}", e));
        }
        // The engine shows a server's error page as the page, which is what
        // the app's user should see. A capture is of the site, not of its
        // error page: it fails as it did when the engine refused the
        // response, with the same message first, so no board counts a 403
        // as a load.
        if let Some(status) = engine
            .http_status(view_id)
            .filter(|s| !(200..300).contains(s))
        {
            return result.failed(
                "error",
                format!("Failed to load URL: NavigationError(\"HTTP error\") (HTTP {status}; the engine rendered its body)"),
            );
        }
        if let Some(log) = engine.script_log(view_id) {
            result.script_stats = Some(script_stats(log));
            if let Some(ref path) = args.dump_scripts {
                if let Err(e) = fs::write(path, script_log_json(log).to_string()) {
                    error!("Failed to write script log: {:?}", e);
                }
            }
        }
        if let Some(live_ms) = args.live_ms {
            result.live_stats = Some(run_live(&rt, &mut engine, view_id, live_ms));
        }
    } else if let Some(html) = html_content {
        if let Err(e) = engine.load_html(view_id, &html) {
            return result.failed("error", format!("Failed to load HTML: {:?}", e));
        }
        // With script on, `load_html` ran the inline scripts.
        if args.run_inline_scripts {
            let log = engine.script_log(view_id).unwrap_or_default();
            if let Some(failed) = inline_script_failure(&log) {
                return result.failed(
                    "error",
                    format!("Inline script failed: {} {:?}", failed.source, failed.outcome),
                );
            }
        }
    }

    if let Some((width, height)) = args.resize_to {
        let bounds = Bounds {
            x: 0,
            y: 0,
            width,
            height,
        };
        if let Err(e) = engine.resize_view(view_id, bounds) {
            return result.failed("error", format!("Failed to resize view: {:?}", e));
        }
        result.width = width;
        result.height = height;
    }

    if let Some(ref actions_raw) = args.actions {
        let actions = match parse_actions(actions_raw) {
            Ok(acts) => acts,
            Err(e) => {
                let _ = engine.destroy_view(view_id);
                return result.failed("error", e);
            }
        };
        execute_actions(
            &mut engine,
            view_id,
            &actions,
            args.actions_out_dir.as_deref(),
            &mut result,
        );
    }

    // Render final view
    if let Err(e) = engine.render_view(view_id) {
        return result.failed("error", format!("Failed to render: {:?}", e));
    }

    // Capture frame if requested
    if let Some(ref path) = args.dump_frame {
        if let Err(e) = engine.capture_frame(view_id, path) {
            error!("Failed to capture frame: {:?}", e);
        } else {
            result.frame_path = Some(path.clone());
        }
    }

    // Export layout if requested
    if let Some(ref path) = args.dump_layout {
        match engine.export_layout_json(view_id, path) {
            Ok(()) => {
                result.layout_path = Some(path.clone());
                // Read back the file to analyze
                match fs::read_to_string(path) {
                    Ok(layout_json) => result.layout_stats = analyze_layout_json(&layout_json),
                    Err(e) => error!("Failed to read layout file: {:?}", e),
                }
            }
            Err(e) => error!("Failed to export layout: {:?}", e),
        }
    }

    // Export display list if requested
    if let Some(ref path) = args.dump_display_list {
        match engine.export_display_list_json(view_id, path) {
            Ok(()) => result.display_list_path = Some(path.clone()),
            Err(e) => error!("Failed to export display list: {:?}", e),
        }
    }

    write_diagnostics(&engine, view_id, args, &mut result);

    if args.inspector_out.is_some() {
        let opts = rustkit_engine::DiagnosticsOptions {
            include_url_query: args.diagnostics_keep_query,
            max_elements: args.diagnostics_max_elements,
        };
        let dump = engine
            .all_element_diagnostics(view_id, &opts)
            .map_err(|e| format!("Failed to build diagnostics: {e:?}"))
            .and_then(|d| serde_json::to_string(&d).map_err(|e| e.to_string()));
        let select = match args.inspector_select_at {
            Some((x, y)) => match engine.inspector_selection_at(view_id, x, y) {
                Ok(sel) => sel.map(|s| s.node_id),
                Err(e) => return result.failed("error", format!("Inspector selection failed: {e:?}")),
            },
            None => None,
        };
        if let Err(e) = dump.and_then(|d| write_inspector(args, &d, select, &mut result)) {
            return result.failed("error", e);
        }
    }

    // Clean up
    let _ = engine.destroy_view(view_id);

    result
}

/// `--diagnose`, `--diagnose-at` and `--dump-diagnostics`: records read from
/// the final layout, after any actions and the final render.
fn write_diagnostics(
    engine: &rustkit_engine::Engine,
    view_id: rustkit_engine::EngineViewId,
    args: &Args,
    result: &mut CaptureResult,
) {
    use rustkit_engine::{DiagnosticTarget, DiagnosticsOptions};
    let opts = DiagnosticsOptions {
        include_url_query: args.diagnostics_keep_query,
        max_elements: args.diagnostics_max_elements,
    };
    let targets: Vec<DiagnosticTarget> = args
        .diagnose
        .iter()
        .map(|s| DiagnosticTarget::Selector(s.clone()))
        .chain(args.diagnose_at.iter().map(|&(x, y)| DiagnosticTarget::Point { x, y }))
        .collect();
    if !targets.is_empty() {
        let records = targets
            .iter()
            .map(|t| match engine.element_diagnostics(view_id, t, &opts) {
                Ok(r) => serde_json::to_value(r).unwrap_or(serde_json::Value::Null),
                Err(e) => serde_json::json!({ "error": format!("{e:?}") }),
            })
            .collect();
        result.diagnostics = Some(records);
    }
    if let Some(ref path) = args.dump_diagnostics {
        match engine.all_element_diagnostics(view_id, &opts) {
            Ok(dump) => {
                if dump.truncated {
                    warn!(
                        total = dump.total_elements,
                        written = dump.elements.len(),
                        "diagnostics dump truncated at --diagnostics-max-elements"
                    );
                }
                match serde_json::to_string(&dump).map(|json| fs::write(path, json)) {
                    Ok(Ok(())) => result.diagnostics_path = Some(path.clone()),
                    Ok(Err(e)) => error!("Failed to write diagnostics: {:?}", e),
                    Err(e) => error!("Failed to serialise diagnostics: {:?}", e),
                }
            }
            Err(e) => error!("Failed to build diagnostics: {:?}", e),
        }
    }
    if args.report.is_some() || args.report_text.is_some() {
        result.page_report = layout_report(engine, view_id, &opts);
    }
}

/// The most elements the page report reads. A consent dialog is often the
/// last thing in the body, so the report reads past the dump's default cap.
const REPORT_MAX_ELEMENTS: usize = 20_000;

/// The layout half of `--report`, from the final layout.
fn layout_report(
    engine: &rustkit_engine::Engine,
    view_id: rustkit_engine::EngineViewId,
    opts: &rustkit_engine::DiagnosticsOptions,
) -> Option<report::Report> {
    use rustkit_engine::{DiagnosticTarget, DiagnosticsOptions};
    let opts = DiagnosticsOptions { max_elements: opts.max_elements.max(REPORT_MAX_ELEMENTS), ..opts.clone() };
    let dump = match engine.all_element_diagnostics(view_id, &opts).map(serde_json::to_value) {
        Ok(Ok(dump)) => dump,
        Ok(Err(e)) => {
            error!("Failed to serialise the report's diagnostics: {:?}", e);
            return None;
        }
        Err(e) => {
            error!("Failed to build the report's diagnostics: {:?}", e);
            return None;
        }
    };
    let (w, h) = (
        dump["page"]["viewport"][0].as_f64().unwrap_or(0.0) as f32,
        dump["page"]["viewport"][1].as_f64().unwrap_or(0.0) as f32,
    );
    let centre = engine
        .element_diagnostics(view_id, &DiagnosticTarget::Point { x: w / 2.0, y: h / 2.0 }, &opts)
        .ok()
        .flatten()
        .and_then(|r| serde_json::to_value(r).ok());
    let mut page = report::page_report(&dump, centre.as_ref());
    if dump["truncated"].as_bool() == Some(true) {
        page.not_checked.push(format!("elements past the first {} (the page has {})", opts.max_elements, page.elements));
    }
    Some(page)
}

/// `--report` and `--report-text`.
fn write_report_files(args: &Args, page: &report::Report) {
    if let Some(ref path) = args.report {
        match serde_json::to_string_pretty(&page).map(|json| fs::write(path, json)) {
            Ok(Ok(())) => {}
            Ok(Err(e)) => error!("Failed to write the report: {:?}", e),
            Err(e) => error!("Failed to serialise the report: {:?}", e),
        }
    }
    if let Some(ref path) = args.report_text {
        if let Err(e) = fs::write(path, report::report_text(&page)) {
            error!("Failed to write the report text: {:?}", e);
        }
    }
}

/// `--inspector-out`: the Inspector for `dump`, with `--inspector-diff`.
fn write_inspector(args: &Args, dump: &str, select: Option<usize>, result: &mut CaptureResult) -> Result<(), String> {
    let Some(ref out) = args.inspector_out else { return Ok(()) };
    let diff = match args.inspector_diff {
        Some(ref p) => Some(fs::read_to_string(p).map_err(|e| format!("Failed to read {p}: {e}"))?),
        None => None,
    };
    let written = inspector::render_inspector(dump, diff.as_deref(), select, args.inspector_size, Path::new(out))
        .map_err(|e| format!("Inspector: {e}"))?;
    result.inspector = Some(written);
    Ok(())
}

/// `X,Y` in CSS px.
fn parse_point_arg(s: &str) -> Result<(f32, f32), String> {
    let (x, y) = s
        .split_once(',')
        .ok_or_else(|| format!("expected X,Y, got {s:?}"))?;
    let x = x.trim().parse().map_err(|e| format!("x {x:?}: {e}"))?;
    let y = y.trim().parse().map_err(|e| format!("y {y:?}: {e}"))?;
    Ok((x, y))
}

fn parse_actions(raw: &str) -> Result<Vec<ActionItem>, String> {
    let trimmed = raw.trim();
    let content = if trimmed.starts_with('[') || trimmed.starts_with('{') {
        trimmed.to_string()
    } else {
        fs::read_to_string(trimmed).map_err(|e| format!("failed to read actions file: {e}"))?
    };
    let content = content.trim();
    if content.starts_with('[') {
        serde_json::from_str::<Vec<ActionItem>>(content)
            .map_err(|e| format!("failed to parse actions JSON array: {e}"))
    } else {
        serde_json::from_str::<ActionsPayload>(content)
            .map(|p| p.actions)
            .map_err(|e| format!("failed to parse actions JSON object: {e}"))
    }
}

fn parse_point(eval_res: &str) -> Option<(f32, f32)> {
    let idx = eval_res.find("point:")?;
    let rest = &eval_res[idx + 6..];
    let parts: Vec<&str> = rest.split(':').collect();
    if parts.len() < 2 {
        return None;
    }
    let x_part = parts[0].trim_matches(|c: char| !c.is_ascii_digit() && c != '.' && c != '-');
    let y_part = parts[1].trim_matches(|c: char| !c.is_ascii_digit() && c != '.' && c != '-');
    let x = x_part.parse::<f32>().ok()?;
    let y = y_part.parse::<f32>().ok()?;
    Some((x, y))
}

fn execute_actions(
    engine: &mut rustkit_engine::Engine,
    view_id: rustkit_engine::EngineViewId,
    actions: &[ActionItem],
    actions_out_dir: Option<&str>,
    result: &mut CaptureResult,
) {
    let mut captures = Vec::new();
    let mut action_results = Vec::new();

    for (i, a) in actions.iter().enumerate() {
        let step = a.step.unwrap_or(i);
        let start = Instant::now();
        let mut a_res = ActionResult {
            step,
            action_type: a.action_type.clone(),
            status: "ok".to_string(),
            error: None,
            frame: None,
            label: a.label.clone(),
            selector_used: None,
            method: None,
            elapsed_ms: 0,
        };

        match a.action_type.as_str() {
            "wait" => {
                if let Some(ref sel) = a.selector {
                    let timeout = Duration::from_millis(a.timeout_ms.or(a.ms).unwrap_or(2000));
                    let wait_start = Instant::now();
                    let sel_json = serde_json::to_string(sel).unwrap_or_default();
                    let check_js = format!(
                        "Boolean(document.querySelector({})) ? 'found' : 'missing'",
                        sel_json
                    );
                    let mut found = false;
                    while wait_start.elapsed() < timeout {
                        if let Ok(eval_res) = engine.execute_script(view_id, &check_js) {
                            if eval_res.contains("found") && !eval_res.contains("missing") {
                                found = true;
                                break;
                            }
                        }
                        std::thread::sleep(Duration::from_millis(20));
                    }
                    if !found {
                        a_res.status = "timeout".to_string();
                        a_res.error = Some(format!("timed out waiting for selector {}", sel));
                    }
                } else {
                    let ms = a.ms.unwrap_or(500);
                    std::thread::sleep(Duration::from_millis(ms));
                }
            }
            "click" => {
                if let Some(ref sel) = a.selector {
                    let selectors: Vec<&str> = sel.split(',').map(|s| s.trim()).collect();
                    let mut clicked = false;
                    for s in selectors {
                        let s_json = serde_json::to_string(s).unwrap_or_default();
                        let query_js = format!(
                            r#"(function() {{
                                try {{
                                    var el = document.querySelector({0});
                                    if (!el) return 'missing';
                                    if (typeof el.getBoundingClientRect === 'function') {{
                                        var r = el.getBoundingClientRect();
                                        if (r && r.width > 0 && r.height > 0) {{
                                            var cx = r.left + r.width / 2.0;
                                            var cy = r.top + r.height / 2.0;
                                            return 'point:' + cx + ':' + cy;
                                        }}
                                    }}
                                    if (typeof el.focus === 'function') {{
                                        try {{ el.focus(); }} catch (e) {{}}
                                    }}
                                    if (typeof el.click === 'function') {{
                                        try {{ el.click(); }} catch (e) {{}}
                                    }} else {{
                                        try {{ el.dispatchEvent(new Event('click', {{ bubbles: true, cancelable: true }})); }} catch (e) {{}}
                                    }}
                                    return 'fallback_clicked';
                                }} catch (e) {{
                                    return 'error:' + e;
                                }}
                            }})()"#,
                            s_json
                        );
                        if let Ok(eval_res) = engine.execute_script(view_id, &query_js) {
                            if let Some((cx, cy)) = parse_point(&eval_res) {
                                engine.mouse_move_at_point(view_id, cx, cy);
                                let _ = engine.mouse_down_at_point(view_id, cx, cy);
                                let _ = engine.click_at_point(view_id, cx, cy);
                                let _ = engine.relayout(view_id);
                                clicked = true;
                                a_res.selector_used = Some(s.to_string());
                                a_res.method = Some(format!("engine_point({:.1}, {:.1})", cx, cy));
                                break;
                            }
                            if eval_res.contains("fallback_clicked") {
                                let _ = engine.relayout(view_id);
                                clicked = true;
                                a_res.selector_used = Some(s.to_string());
                                a_res.method = Some("synthetic_fallback".to_string());
                                if a_res.label.is_none() {
                                    a_res.label = Some("fallback".to_string());
                                }
                                break;
                            }
                        }
                    }
                    if !clicked {
                        a_res.status = "selector_not_found".to_string();
                        a_res.error =
                            Some(format!("no matching element found for selector {}", sel));
                    }
                } else if let (Some(x), Some(y)) = (a.x, a.y) {
                    engine.mouse_move_at_point(view_id, x, y);
                    let _ = engine.mouse_down_at_point(view_id, x, y);
                    let _ = engine.click_at_point(view_id, x, y);
                    let _ = engine.relayout(view_id);
                    a_res.method = Some(format!("engine_point({:.1}, {:.1})", x, y));
                } else {
                    a_res.status = "error".to_string();
                    a_res.error = Some("click action requires selector or (x, y)".to_string());
                }
            }
            "key" => {
                if let Some(ref sel) = a.selector {
                    let sel_json = serde_json::to_string(sel).unwrap_or_default();
                    let focus_js = format!(
                        "var el = document.querySelector({}); if (el && typeof el.focus === 'function') el.focus();",
                        sel_json
                    );
                    let _ = engine.execute_script(view_id, &focus_js);
                }
                if let Some(ref text) = a.text {
                    let text_json = serde_json::to_string(text).unwrap_or_default();
                    let key_js = format!(
                        r#"(function() {{
                            var el = document.activeElement;
                            if (el && 'value' in el) {{
                                el.value = (el.value || '') + {0};
                                el.dispatchEvent(new Event('input', {{ bubbles: true }}));
                                el.dispatchEvent(new Event('change', {{ bubbles: true }}));
                            }}
                        }})()"#,
                        text_json
                    );
                    let _ = engine.execute_script(view_id, &key_js);
                    let _ = engine.relayout(view_id);
                } else if let Some(ref key) = a.key {
                    let key_code = match key.as_str() {
                        "Enter" => 13,
                        "Escape" => 27,
                        "Backspace" => 8,
                        "Tab" => 9,
                        _ => 0,
                    };
                    let _ = engine.handle_text_key(view_id, key_code, key, false, false, false);
                    let key_json = serde_json::to_string(key).unwrap_or_default();
                    let event_js = format!(
                        r#"(function() {{
                            var el = document.activeElement || document.body;
                            if (el) {{
                                el.dispatchEvent(new KeyboardEvent('keydown', {{ key: {0}, bubbles: true }}));
                                el.dispatchEvent(new KeyboardEvent('keyup', {{ key: {0}, bubbles: true }}));
                            }}
                        }})()"#,
                        key_json
                    );
                    let _ = engine.execute_script(view_id, &event_js);
                    let _ = engine.relayout(view_id);
                }
            }
            "resize" => {
                if let (Some(w), Some(h)) = (a.width, a.height) {
                    let bounds = Bounds {
                        x: 0,
                        y: 0,
                        width: w,
                        height: h,
                    };
                    if let Err(e) = engine.resize_view(view_id, bounds) {
                        a_res.status = "error".to_string();
                        a_res.error = Some(format!("resize failed: {:?}", e));
                    } else {
                        result.width = w;
                        result.height = h;
                        let _ = engine.relayout(view_id);
                    }
                } else {
                    a_res.status = "error".to_string();
                    a_res.error = Some("resize requires width and height".to_string());
                }
            }
            "capture" => {
                if let Some(ref frame) = a.frame {
                    let frame_path = match actions_out_dir {
                        Some(out_dir) => {
                            Path::new(out_dir).join(frame).to_string_lossy().to_string()
                        }
                        None => frame.clone(),
                    };
                    if let Some(parent) = Path::new(&frame_path).parent() {
                        let _ = fs::create_dir_all(parent);
                    }
                    let _ = engine.render_view(view_id);
                    if let Err(e) = engine.capture_frame(view_id, &frame_path) {
                        a_res.status = "error".to_string();
                        a_res.error = Some(format!("capture_frame failed: {:?}", e));
                    } else {
                        let label = a.label.clone().unwrap_or_else(|| format!("step_{}", step));
                        captures.push(StepCapture {
                            step,
                            label: label.clone(),
                            frame: frame_path.clone(),
                        });
                        a_res.frame = Some(frame_path);
                        a_res.label = Some(label);
                    }
                } else {
                    a_res.status = "error".to_string();
                    a_res.error = Some("capture requires frame path".to_string());
                }
            }
            other => {
                a_res.status = "error".to_string();
                a_res.error = Some(format!("unknown action type: {}", other));
            }
        }

        a_res.elapsed_ms = start.elapsed().as_millis() as u64;
        action_results.push(a_res);
    }

    result.captures = Some(captures);
    result.action_results = Some(action_results);
}

/// `WxH`, e.g. `1024x768`.
fn parse_size(s: &str) -> Result<(u32, u32), String> {
    let (w, h) = s
        .split_once('x')
        .ok_or_else(|| format!("expected WxH, got {s:?}"))?;
    let w = w.parse().map_err(|e| format!("width {w:?}: {e}"))?;
    let h = h.parse().map_err(|e| format!("height {h:?}: {e}"))?;
    Ok((w, h))
}

fn script_stats(log: &[ScriptRecord]) -> ScriptStats {
    let mut stats = ScriptStats {
        ran: 0,
        threw: 0,
        skipped: 0,
        fetch_failed: 0,
        over_budget: 0,
        bytes: 0,
        elapsed_ms: 0,
    };
    for record in log {
        match record.outcome {
            ScriptOutcome::Ran => stats.ran += 1,
            ScriptOutcome::Threw(_) => stats.threw += 1,
            ScriptOutcome::Skipped(_) | ScriptOutcome::Nomodule => stats.skipped += 1,
            ScriptOutcome::FetchFailed(_) => stats.fetch_failed += 1,
            ScriptOutcome::OverBudget => stats.over_budget += 1,
            ScriptOutcome::EngineFailed(_) => stats.threw += 1,
        }
        stats.bytes += record.bytes as u64;
        stats.elapsed_ms += record.elapsed_ms;
    }
    stats
}

fn script_log_json(log: &[ScriptRecord]) -> serde_json::Value {
    let records: Vec<_> = log
        .iter()
        .map(|r| {
            let (outcome, detail) = match &r.outcome {
                ScriptOutcome::Ran => ("ran", None),
                ScriptOutcome::Threw(m) => ("threw", Some(m.clone())),
                ScriptOutcome::Skipped(why) => ("skipped", Some(why.to_string())),
                ScriptOutcome::Nomodule => (
                    "skipped",
                    Some("nomodule (skipped, as in module-capable browsers)".to_string()),
                ),
                ScriptOutcome::FetchFailed(m) => ("fetch_failed", Some(m.to_string())),
                ScriptOutcome::OverBudget => ("over_budget", None),
                // Reported as before the engine marked its own failures.
                ScriptOutcome::EngineFailed(failure) => ("threw", Some(failure.to_string())),
            };
            serde_json::json!({
                "source": r.source,
                "bytes": r.bytes,
                "elapsed_ms": r.elapsed_ms,
                "outcome": outcome,
                "detail": detail,
            })
        })
        .collect();
    serde_json::json!({ "scripts": records })
}

/// Mirror the Chrome capture pipeline's CSS inputs for a file loaded via
/// `Engine::load_html`, which uses a synthetic about:blank base URL and never
/// fetches subresources:
///
/// 1. Inline every `<link rel="stylesheet" href="...">` whose href resolves to
///    a file relative to the HTML file's directory (Chrome loads these
///    natively over file://).
/// 2. For micro-suite fixtures, inject `baselines/common/parity-reset.css` as
///    the FIRST style in `<head>` — Chrome's capture injects it via an init
///    script before the fixture's own styles, so fixture rules win ties there
///    and must win ties here too.
fn preprocess_html(html: &str, html_path: &Path) -> String {
    let base_dir = html_path.parent().unwrap_or_else(|| Path::new("."));
    let mut out = inline_stylesheet_links(html, base_dir);

    if is_micro_suite_path(html_path) {
        if let Some(reset_css) = read_repo_parity_reset(html_path) {
            out = inject_style_first_in_head(&out, "data-parity-reset=\"1\"", &reset_css);
        } else {
            warn!("micro-suite fixture but baselines/common/parity-reset.css not found");
        }
    }

    out
}

fn is_micro_suite_path(html_path: &Path) -> bool {
    // Canonicalize first: the separator-delimited patterns below require a
    // LEADING separator, so a repo-relative invocation ("websuite/micro/x")
    // never matched and the reset was silently skipped — while CI passes
    // absolute paths and always matched. Local captures therefore rendered
    // micro cases WITHOUT the reset Chrome's oracle applies (line-height
    // 1.5 vs metrics-normal, ~5px per line), an invisible capture-
    // environment asymmetry between every local board and CI.
    let canon = html_path
        .canonicalize()
        .unwrap_or_else(|_| html_path.to_path_buf());
    let p = canon.to_string_lossy();
    p.contains("/websuite/micro/") || p.contains("\\websuite\\micro\\")
}

/// Walk up from the HTML file to the repo root (the directory containing
/// `baselines/common/parity-reset.css`) and read the reset, matching
/// deterministic.mjs's RESET_CSS_PATH.
fn read_repo_parity_reset(html_path: &Path) -> Option<String> {
    let mut dir = html_path.parent()?;
    loop {
        let candidate = dir.join("baselines/common/parity-reset.css");
        if candidate.is_file() {
            return fs::read_to_string(candidate).ok();
        }
        dir = dir.parent()?;
    }
}

/// Replace `<link rel="stylesheet" href="...">` tags with inline `<style>`
/// blocks holding the referenced file's contents, preserving document order
/// so the cascade is unchanged. Absolute (scheme://) hrefs and unreadable
/// files keep the original tag and produce a warning.
fn inline_stylesheet_links(html: &str, base_dir: &Path) -> String {
    let lower = html.to_ascii_lowercase();
    let mut out = String::with_capacity(html.len());
    let mut pos = 0;

    while let Some(rel_start) = lower[pos..].find("<link") {
        let tag_start = pos + rel_start;
        let Some(rel_end) = lower[tag_start..].find('>') else {
            break;
        };
        let tag_end = tag_start + rel_end + 1;
        let tag = &html[tag_start..tag_end];

        out.push_str(&html[pos..tag_start]);
        match stylesheet_replacement_for_link_tag(tag, base_dir) {
            Some(style_block) => out.push_str(&style_block),
            None => out.push_str(tag),
        }
        pos = tag_end;
    }
    out.push_str(&html[pos..]);
    out
}

/// If the tag is a resolvable relative stylesheet link, build its inline
/// `<style>` replacement; otherwise None (keep the tag as-is).
fn stylesheet_replacement_for_link_tag(tag: &str, base_dir: &Path) -> Option<String> {
    let rel = extract_attr(tag, "rel")?;
    if !rel
        .split_ascii_whitespace()
        .any(|t| t.eq_ignore_ascii_case("stylesheet"))
    {
        return None;
    }
    let href = extract_attr(tag, "href")?;
    if href.contains("://") || href.starts_with("//") {
        warn!(href, "leaving non-local stylesheet link unresolved");
        return None;
    }

    let css_path = base_dir.join(&href);
    let css = match fs::read_to_string(&css_path) {
        Ok(css) => css,
        Err(e) => {
            warn!(href, ?css_path, ?e, "failed to read linked stylesheet");
            return None;
        }
    };
    if css.to_ascii_lowercase().contains("</style") {
        warn!(href, "stylesheet contains '</style' — cannot inline safely");
        return None;
    }

    let media = extract_attr(tag, "media").unwrap_or_default();
    let needs_media_wrap = !media.is_empty() && !media.eq_ignore_ascii_case("all");
    let body = if needs_media_wrap {
        format!("@media {} {{\n{}\n}}", media, css)
    } else {
        css
    };
    Some(format!(
        "<style data-inlined-href=\"{}\">\n{}\n</style>",
        href, body
    ))
}

/// Crude attribute extraction: name="value" or name='value' (fixtures are
/// controlled inputs; unquoted values are not used there).
fn extract_attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let needle = format!("{}=", name);
    let mut search_from = 0;
    while let Some(found) = lower[search_from..].find(&needle) {
        let idx = search_from + found;
        // Must be preceded by whitespace to be an attribute name boundary.
        let boundary_ok = idx > 0
            && lower[..idx]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_ascii_whitespace());
        let val_start = idx + needle.len();
        if boundary_ok {
            let rest = &tag[val_start..];
            let mut chars = rest.chars();
            return match chars.next() {
                Some(q @ ('"' | '\'')) => {
                    let inner = &rest[1..];
                    inner.find(q).map(|end| inner[..end].to_string())
                }
                _ => None,
            };
        }
        search_from = val_start;
    }
    None
}

/// Insert a `<style {attrs}>` block as the first child of `<head>`, falling
/// back to just after `<html...>` or the start of the document.
fn inject_style_first_in_head(html: &str, attrs: &str, css: &str) -> String {
    let style_block = format!("<style {}>\n{}\n</style>", attrs, css);
    let lower = html.to_ascii_lowercase();

    let insert_at = ["<head", "<html"].iter().find_map(|open| {
        lower
            .find(open)
            .and_then(|start| lower[start..].find('>').map(|end| start + end + 1))
    });

    match insert_at {
        Some(at) => format!("{}{}{}", &html[..at], style_block, &html[at..]),
        None => format!("{}{}", style_block, html),
    }
}

fn analyze_layout_json(json_str: &str) -> Option<LayoutStats> {
    let data: serde_json::Value = serde_json::from_str(json_str).ok()?;

    let mut stats = LayoutStats {
        total_boxes: 0,
        sized: 0,
        zero_size: 0,
        positioned: 0,
        at_origin: 0,
        sizing_rate: 0.0,
        positioning_rate: 0.0,
    };

    fn walk(node: &serde_json::Value, stats: &mut LayoutStats) {
        if let Some(rect) = node.get("content_rect").or(node.get("rect")) {
            let x = rect.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let y = rect.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let w = rect.get("width").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let h = rect.get("height").and_then(|v| v.as_f64()).unwrap_or(0.0);

            stats.total_boxes += 1;

            if x != 0.0 || y != 0.0 {
                stats.positioned += 1;
            } else {
                stats.at_origin += 1;
            }

            if w > 0.0 && h > 0.0 {
                stats.sized += 1;
            } else {
                stats.zero_size += 1;
            }
        }

        if let Some(children) = node.get("children").and_then(|v| v.as_array()) {
            for child in children {
                walk(child, stats);
            }
        }
    }

    if let Some(root) = data.get("root") {
        walk(root, &mut stats);
    }

    let total = stats.total_boxes.max(1) as f32;
    stats.sizing_rate = stats.sized as f32 / total;
    stats.positioning_rate = stats.positioned as f32 / total;

    Some(stats)
}

/// The first script of an `--html-file` load that failed: it threw, the
/// engine failed, or the budget stopped it. A script the HTML load does not
/// run (an external `<script src>`, a module, `nomodule`) is not a failure.
fn inline_script_failure(log: &[ScriptRecord]) -> Option<&ScriptRecord> {
    log.iter().find(|r| {
        matches!(r.outcome, ScriptOutcome::Threw(_) | ScriptOutcome::EngineFailed(_) | ScriptOutcome::OverBudget)
    })
}

/// The engine configuration a capture runs with: the parity defaults, plus
/// whatever the command line overrides.
fn capture_config(args: &Args, replay_proxy: Option<Url>) -> EngineConfig {
    let mut config = EngineConfig::for_parity_testing();
    if let Some(horizon) = args.timer_horizon_ms {
        config.timer_horizon_ms = horizon;
    }
    if let Some(budget) = args.script_budget_ms {
        config.script_budget_ms = budget;
    }
    config.interrupt_scripts_at_budget = args.interrupt_scripts;
    config.replay_proxy = replay_proxy;
    config.inspector.keep_url_query = args.event_log_keep_query;
    config.inspector.keep_console_text = args.event_log_console_text;
    config.inspector.keep_error_text = args.event_log_error_text;
    config
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// #799 review (3): `--run-inline-scripts` fails a capture only on a
    /// script that threw, failed the engine or ran out of budget. An external
    /// `<script src>` or a module the HTML load skips is not one.
    #[test]
    fn run_inline_scripts_fails_only_on_scripts_that_failed() {
        let log_of = |html: &str| {
            let mut engine = EngineBuilder::new().javascript_enabled(true).build().expect("engine");
            let view = engine
                .create_headless_view(Bounds { x: 0, y: 0, width: 200, height: 100 })
                .expect("view");
            engine.load_html(view, html).expect("load_html");
            engine.script_log(view).unwrap_or_default().to_vec()
        };
        let skipped = log_of(
            "<html><body><script src=\"/app.js\"></script>\
<script type=\"module\">window.m = 1;</script><script>var ok = 1;</script></body></html>",
        );
        assert!(skipped.iter().any(|r| matches!(r.outcome, ScriptOutcome::Skipped(_))), "{skipped:?}");
        assert!(inline_script_failure(&skipped).is_none(), "{skipped:?}");
        let threw = log_of("<html><body><script src=\"/app.js\"></script><script>throw new Error('x');</script></body></html>");
        let failed = inline_script_failure(&threw).expect("a thrown script fails the capture");
        assert_eq!(failed.source, "inline#2", "{threw:?}");
    }

    fn parsed(extra: &[&str]) -> Args {
        let mut argv = vec!["parity-capture", "--url", "https://example.test/"];
        argv.extend_from_slice(extra);
        Args::try_parse_from(argv).expect("the command line parses")
    }

    // The live loop is the app's, not the board's: a capture turns it only
    // when asked.
    #[test]
    fn the_live_loop_is_turned_only_when_asked() {
        assert_eq!(parsed(&[]).live_ms, None);
        assert_eq!(parsed(&["--live-ms", "20000"]).live_ms, Some(20_000));
    }

    // The board is measured at the engine's 5 s script budget, and the live
    // app runs pages at 60 s (#573). `--script-budget-ms` lets one labelled
    // run be taken at the app's budget; without the flag nothing changes.
    #[test]
    fn the_script_budget_is_the_engines_default_without_the_flag() {
        let config = capture_config(&parsed(&[]), None);
        assert_eq!(config.script_budget_ms, 5_000);
        assert_eq!(
            config.script_budget_ms,
            EngineConfig::for_parity_testing().script_budget_ms
        );
        assert_eq!(
            config.timer_horizon_ms,
            EngineConfig::for_parity_testing().timer_horizon_ms
        );
    }

    #[test]
    fn a_running_script_is_stopped_at_the_budget_only_with_the_flag() {
        assert!(!capture_config(&parsed(&[]), None).interrupt_scripts_at_budget);
        assert!(!capture_config(&parsed(&["--script-budget-ms", "60000"]), None).interrupt_scripts_at_budget);
        let config = capture_config(&parsed(&["--interrupt-scripts"]), None);
        assert!(config.interrupt_scripts_at_budget);
        assert_eq!(config.script_budget_ms, 5_000);
    }

    #[test]
    fn event_log_urls_lose_their_query_unless_asked() {
        assert!(!capture_config(&parsed(&["--event-log", "x.jsonl"]), None).inspector.keep_url_query);
        assert!(capture_config(&parsed(&["--event-log-keep-query"]), None).inspector.keep_url_query);
        assert!(!capture_config(&parsed(&["--event-log", "x.jsonl"]), None).inspector.keep_console_text);
        assert!(capture_config(&parsed(&["--event-log-console-text"]), None).inspector.keep_console_text);
        assert!(!capture_config(&parsed(&["--event-log", "x.jsonl"]), None).inspector.keep_error_text);
        assert!(capture_config(&parsed(&["--event-log-error-text"]), None).inspector.keep_error_text);
        // The flags add a sink; the buffer is the tab ring (T6b), off with
        // HIWAVE_DEVTOOLS_RING=0.
        let config = capture_config(&parsed(&["--event-summary"]), None);
        assert_eq!(config.inspector.capacity, rustkit_engine::InspectorConfig::tab().capacity);
    }

    /// A fixture capture with both flags: the log is JSON Lines of the
    /// whole load, and the summary counts it.
    #[test]
    fn an_event_log_and_summary_cover_the_whole_load() {
        let dir = std::env::temp_dir().join(format!("parity-capture-events-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let html = dir.join("page.html");
        fs::write(&html, "<html><body>x</body></html>").unwrap();
        let log = dir.join("events.jsonl");
        let args = Args::try_parse_from([
            "parity-capture",
            "--html-file",
            html.to_str().unwrap(),
            "--event-log",
            log.to_str().unwrap(),
            "--event-summary",
        ])
        .unwrap();
        let result = run_capture(&args);
        assert_eq!(result.status, "ok", "{:?}", result.error);
        let text = fs::read_to_string(&log).unwrap();
        let kinds: Vec<String> = text
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap()["type"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(kinds.first().map(String::as_str), Some("navigation_start"));
        assert!(kinds.contains(&"load_finished".to_string()), "{kinds:?}");
        let summary = result.event_summary.expect("a summary");
        assert_eq!(summary.events as usize, kinds.len());
        assert_eq!(summary.schema, rustkit_engine::inspector::SUMMARY_SCHEMA);
        let _ = fs::remove_dir_all(&dir);
    }

    /// A page whose only pending work is an inserted script's fetch (no
    /// timer set, nothing `in_flight`) still runs the script when its fetch
    /// arrives, not on the live window's last turn.
    #[test]
    fn an_inserted_script_runs_when_its_fetch_arrives_not_at_the_end_of_the_window() {
        use std::io::{BufRead, BufReader};
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                std::thread::spawn(move || {
                    let mut line = String::new();
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let _ = reader.read_line(&mut line);
                    loop {
                        let mut header = String::new();
                        if reader.read_line(&mut header).unwrap_or(0) <= 2 {
                            break;
                        }
                    }
                    let (ctype, body) = if line.starts_with("GET /late.js") {
                        std::thread::sleep(Duration::from_millis(300));
                        ("text/javascript", "console.log('late ran');".to_string())
                    } else {
                        let page = "<html><body>x<script>setTimeout(function () { var s = document.createElement('script'); s.src = '/late.js'; document.body.appendChild(s); }, 50);</script></body></html>";
                        ("text/html", page.to_string())
                    };
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                });
            }
        });
        let dir = std::env::temp_dir().join(format!("parity-capture-wake-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let log = dir.join("events.jsonl");
        let url = format!("http://127.0.0.1:{port}/");
        let args = Args::try_parse_from([
            "parity-capture",
            "--url",
            url.as_str(),
            "--live-ms",
            "6000",
            "--event-log",
            log.to_str().unwrap(),
        ])
        .unwrap();
        let result = run_capture(&args);
        assert_eq!(result.status, "ok", "{:?}", result.error);
        let records: Vec<serde_json::Value> = fs::read_to_string(&log)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let at = |pred: &dyn Fn(&serde_json::Value) -> bool| records.iter().find(|r| pred(r)).map(|r| r["t_ms"].as_f64().unwrap());
        let load = at(&|r| r["type"] == "load_finished").expect("load_finished");
        let ran = at(&|r| r["type"] == "console").expect("late.js logged");
        assert!(ran - load < 2_000.0, "late.js ran {:.0} ms after load (the window is 6000 ms)", ran - load);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_script_budget_flag_sets_the_engines_script_budget_and_nothing_else() {
        let config = capture_config(&parsed(&["--script-budget-ms", "60000"]), None);
        assert_eq!(config.script_budget_ms, 60_000);
        assert_eq!(
            config.timer_horizon_ms,
            EngineConfig::for_parity_testing().timer_horizon_ms
        );
    }

    fn write_file(dir: &Path, rel: &str, content: &str) {
        let path = dir.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut f = fs::File::create(path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
    }

    #[test]
    fn inlines_relative_stylesheet_link_in_place() {
        let dir = std::env::temp_dir().join("pc-test-inline");
        let _ = fs::remove_dir_all(&dir);
        write_file(&dir, "common/reset.css", "h1 { color: red; }");
        let html = r#"<html><head><link rel="stylesheet" href="common/reset.css"><style>h1{color:blue}</style></head></html>"#;

        let out = inline_stylesheet_links(html, &dir);

        assert!(!out.contains("<link"), "link tag should be replaced");
        assert!(out.contains("h1 { color: red; }"));
        // Order preserved: inlined sheet before the fixture's own <style>.
        let inlined = out.find("color: red").unwrap();
        let fixture = out.find("color:blue").unwrap();
        assert!(inlined < fixture);
    }

    #[test]
    fn leaves_remote_and_missing_links_alone() {
        let dir = std::env::temp_dir().join("pc-test-remote");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let html = concat!(
            r#"<link rel="stylesheet" href="https://example.com/a.css">"#,
            r#"<link rel="stylesheet" href="missing.css">"#,
            r#"<link rel="icon" href="fav.ico">"#,
        );

        let out = inline_stylesheet_links(html, &dir);
        assert_eq!(out, html);
    }

    #[test]
    fn wraps_media_scoped_links() {
        let dir = std::env::temp_dir().join("pc-test-media");
        let _ = fs::remove_dir_all(&dir);
        write_file(&dir, "print.css", "body { display: none; }");
        let html = r#"<link rel="stylesheet" href="print.css" media="print">"#;

        let out = inline_stylesheet_links(html, &dir);
        assert!(out.contains("@media print {"));
    }

    #[test]
    fn injects_reset_first_in_head() {
        let html = "<html><head><style>b{}</style></head></html>";
        let out = inject_style_first_in_head(html, "data-parity-reset=\"1\"", "x{}");
        let reset = out.find("data-parity-reset").unwrap();
        let fixture = out.find("<style>b{}").unwrap();
        assert!(reset < fixture);
    }

    #[test]
    fn micro_suite_detection_accepts_relative_paths() {
        // The predicate must not depend on how the caller spelled the path:
        // CI passes absolute, humans pass relative, and the reset injection
        // silently diverging between them is a capture-environment asymmetry
        // (see the canonicalize comment on is_micro_suite_path).
        // Canonicalization needs a real file, so use one from the tree.
        let real = Path::new("websuite/micro/gradients/index.html");
        if real.exists() {
            assert!(
                is_micro_suite_path(real),
                "relative micro path must be detected"
            );
        }
        assert!(!is_micro_suite_path(Path::new(
            "websuite/cases/x/index.html"
        )));
    }

    #[test]
    fn micro_suite_detection_matches_deterministic_mjs() {
        assert!(is_micro_suite_path(Path::new(
            "/repo/websuite/micro/bg-solid/index.html"
        )));
        assert!(!is_micro_suite_path(Path::new(
            "/repo/websuite/pages/blog/index.html"
        )));
    }

    #[test]
    fn inspector_flags_parse() {
        let argv = |extra: &[&str]| {
            let mut v = vec!["parity-capture"];
            v.extend_from_slice(extra);
            Args::try_parse_from(v)
        };
        // A dump file is a source of its own; it needs an output directory
        // and no page.
        let a = argv(&["--inspector-dump", "d.json", "--inspector-out", "o"]).unwrap();
        assert_eq!((a.inspector_dump.as_deref(), a.inspector_size), (Some("d.json"), (1280, 800)));
        assert!(argv(&["--inspector-dump", "d.json"]).is_err());
        assert!(argv(&["--inspector-dump", "d.json", "--inspector-out", "o", "--html-file", "x.html"]).is_err());
        // A point selects in a captured page, not in a dump file.
        assert!(argv(&["--inspector-dump", "d.json", "--inspector-out", "o", "--inspector-select-at", "1,2"]).is_err());
        let b = parsed(&["--inspector-out", "o", "--inspector-select-at", "3,4.5", "--inspector-size", "800x600"]);
        assert_eq!((b.inspector_select_at, b.inspector_size), (Some((3.0, 4.5)), (800, 600)));
        assert!(Args::try_parse_from(["parity-capture", "--url", "https://e.test/", "--inspector-diff", "x"]).is_err());
        assert!(!parsed(&[]).run_inline_scripts);
        assert!(parsed(&["--run-inline-scripts"]).run_inline_scripts);
    }

    /// The fixture dump and diff (fixtures/inspector), rendered as the
    /// parity case's page is.
    #[test]
    fn the_inspector_renders_a_dump_file_to_png_and_display_list() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/inspector");
        let dump = fs::read_to_string(root.join("target.planted.dump.json")).unwrap();
        let diff = fs::read_to_string(root.join("f2_planted.diff.json")).unwrap();
        let out = std::env::temp_dir().join(format!("parity-capture-inspector-{}", std::process::id()));
        let written = inspector::render_inspector(&dump, Some(&diff), None, (1280, 800), &out).unwrap();
        assert_eq!((written.elements, written.truncated), (21, false));
        assert_eq!(fs::read_to_string(&written.html_path).unwrap(), fs::read_to_string(root.join("inspector_page.html")).unwrap());
        let png = fs::read(&written.png_path).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!((u32::from_be_bytes(png[16..20].try_into().unwrap()), u32::from_be_bytes(png[20..24].try_into().unwrap())), (1280, 800));
        let list: serde_json::Value = serde_json::from_str(&fs::read_to_string(&written.display_list_path).unwrap()).unwrap();
        let texts: Vec<&str> = list["commands"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|c| (c["op"] == "text").then(|| c["text"].as_str().unwrap().trim()))
            .collect();
        for want in ["HiWave Inspector", "first difference", "Difference from Chromium (STYLE)", "flex-grow"] {
            assert!(texts.contains(&want), "{want:?} not painted: {texts:?}");
        }
        assert!(!out.join("inspector.ppm").exists(), "the intermediate frame is removed");
        let _ = fs::remove_dir_all(&out);
    }

    #[test]
    fn diagnostics_flags_parse() {
        let args = parsed(&["--diagnose", "#a", "--diagnose", "div > p", "--diagnose-at", "10,20.5"]);
        assert_eq!(args.diagnose, vec!["#a".to_string(), "div > p".to_string()]);
        assert_eq!(args.diagnose_at, vec![(10.0, 20.5)]);
        assert_eq!(args.diagnostics_max_elements, 5000);
        assert!(args.dump_diagnostics.is_none());
        assert!(Args::try_parse_from(["parity-capture", "--url", "https://e.test/", "--diagnose-at", "10"]).is_err());
    }

    #[test]
    fn parse_point_extracts_coordinates_from_js_string() {
        assert_eq!(
            parse_point("String(\"point:100:200\")"),
            Some((100.0, 200.0))
        );
        assert_eq!(parse_point("point:12.5:30.25"), Some((12.5, 30.25)));
        assert_eq!(parse_point("point:-10.0:40.5"), Some((-10.0, 40.5)));
        assert_eq!(parse_point("String(\"fallback_clicked\")"), None);
        assert_eq!(parse_point("missing"), None);
    }

    #[test]
    fn the_content_thread_flag_selects_the_page_worker() {
        let (name, stack) = capture_worker_spec(false);
        assert_eq!((name, stack), ("capture-worker", 16 * 1024 * 1024));
        let (name, stack) = capture_worker_spec(true);
        assert_eq!(name, "page");
        assert!(stack >= 8 * 1024 * 1024, "content stack {stack}");
        let got = std::thread::Builder::new()
            .name(name.into())
            .stack_size(stack)
            .spawn(|| std::thread::current().name().unwrap().to_string())
            .unwrap()
            .join()
            .unwrap();
        assert_eq!(got, "page");
    }

    /// `--report` and `--report-text` on a fixture: the report JSON and text.
    fn report_for(name: &str, html: &str) -> (serde_json::Value, String) {
        let dir = std::env::temp_dir().join(format!("parity-capture-report-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let page = dir.join("page.html");
        fs::write(&page, html).unwrap();
        let (json, text) = (dir.join("report.json"), dir.join("report.txt"));
        let args = Args::try_parse_from([
            "parity-capture",
            "--html-file",
            page.to_str().unwrap(),
            "--report",
            json.to_str().unwrap(),
            "--report-text",
            text.to_str().unwrap(),
        ])
        .unwrap();
        let result = run_capture(&args);
        assert_eq!(result.status, "ok", "{:?}", result.error);
        let report = serde_json::from_str(&fs::read_to_string(&json).expect("report written")).unwrap();
        let text = fs::read_to_string(&text).expect("report text written");
        let _ = fs::remove_dir_all(&dir);
        (report, text)
    }

    fn causes(report: &serde_json::Value) -> Vec<String> {
        report["ranked"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| format!("{} {}", c["rule"].as_str().unwrap(), c["line"].as_str().unwrap()))
            .collect()
    }

    #[test]
    fn a_fixed_dialog_over_a_locked_body_is_the_first_cause() {
        let (report, text) = report_for(
            "lock",
            "<html><head><style>body { margin: 0; overflow: hidden } .story { height: 3000px }
#consent { position: fixed; top: 0; left: 0; right: 0; bottom: 0; z-index: 9999; background: white }</style></head>
<body><div class=\"story\"><p>story</p></div><div id=\"consent\" class=\"ot-sdk banner\"><p>cookies</p></div></body></html>",
        );
        assert_eq!(
            causes(&report),
            vec![
                "R1 scroll locked (body overflow-y:hidden) by overlay: fixed div#consent.ot-sdk.banner (z 9999) covers 100% of the viewport",
                // The fixture's story is one empty 3000px box.
                "R9 below the fold is empty from y=800 (document 3016px tall): no text or image starts in 800..1600",
            ],
            "{text}"
        );
        assert!(text.contains("1. [R1] scroll locked"), "{text}");
    }

    /// cnn, 2026-10-24: the consent script makes the body `position: fixed`
    /// with `overflow: hidden`. The body is the lock, not the overlay, and
    /// its content is still the page's; a bottom bar smaller than the
    /// covering share is what the lock names.
    #[test]
    fn a_fixed_body_is_the_lock_and_the_largest_fixed_box_is_named() {
        let (report, text) = report_for(
            "fixed-body",
            "<html><head><style>body { margin: 0; position: fixed; overflow: hidden; width: 100% }
p { height: 300px; margin: 0 } #bar { position: fixed; left: 0; right: 0; bottom: 0; height: 160px; z-index: 50; background: white }</style></head>
<body><p>one</p><p>two</p><p>three</p><p>four</p><p>five</p><p>six</p><div style=\"position: fixed; left: 0; right: 0; bottom: 0; height: 160px; z-index: 50; background: white\"><p>cookies</p></div></body></html>",
        );
        // No id or class: named by its structural selector's last steps.
        assert_eq!(
            causes(&report),
            vec!["R1 scroll locked (body overflow-y:hidden) by overlay: fixed body:nth-child(2) > div:nth-child(7) (z 50) covers 20% of the viewport"],
            "{text}"
        );
        assert_eq!(report["covering"], serde_json::json!([]), "{text}");
        assert_eq!(report["bands"][1]["text"], 3, "{text}");
    }

    #[test]
    fn an_empty_viewport_band_below_content_is_r9() {
        let (report, text) = report_for(
            "band",
            "<html><body style=\"margin: 0\"><p>top</p><div style=\"height: 2400px\"></div><p>end</p></body></html>",
        );
        assert_eq!(
            causes(&report),
            vec!["R9 below the fold is empty from y=800 (document 2500px tall): no text or image starts in 800..1600"],
            "{text}"
        );
    }

    #[test]
    fn a_page_with_nothing_to_read_in_the_first_viewport_is_r10() {
        let (report, text) = report_for("blank", "<html><body><div style=\"height:10px\"></div></body></html>");
        let causes = causes(&report);
        assert_eq!(causes.len(), 1, "{text}");
        assert!(causes[0].starts_with("R10 the first viewport has no text and no image ("), "{text}");
    }

    #[test]
    fn a_short_plain_page_fires_no_rule_and_says_what_it_did_not_check() {
        let (report, text) = report_for("plain", "<html><body><p>a</p><img src=\"data:,\" width=\"10\" height=\"10\"></body></html>");
        assert_eq!(causes(&report), Vec::<String>::new(), "{text}");
        assert!(text.contains("(no rule fired)"), "{text}");
        // develop has no T2/T3 records: those rules are named as skipped.
        let skipped: Vec<&str> = report["not_checked"].as_array().unwrap().iter().map(|n| n.as_str().unwrap()).collect();
        assert!(skipped.iter().any(|n| n.starts_with("R3, R4")) && skipped.iter().any(|n| n.starts_with("R7")), "{skipped:?}");
    }
}
