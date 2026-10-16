//! The engine half of the script-network bridge.
//!
//! Bindings queue what a page script asks for (`web_net_bridge.js`); this
//! module takes the queue, runs every request under the page's
//! [`FetchPolicy`] (the one allow/deny layer, in rustkit-net) through the
//! engine's `ResourceLoader`, and delivers each outcome back. It adds no
//! rule of its own: a request the policy refuses is an error to the page,
//! and the page sees only "network error", never the reason (which would
//! tell a script what is on the user's network).
//!
//! Determinism: requests are taken in the order the page made them,
//! fetched concurrently (the policy bounds the concurrency), and delivered
//! in that same order, so a run does not depend on which response was
//! fastest.

use std::future::Future;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::pin::Pin;
use std::sync::{Arc, Mutex, OnceLock};
use std::task::Poll;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use bytes::Bytes;
use http::{HeaderMap, HeaderName, HeaderValue, Method};
use rustkit_bindings::{DomBindings, NetDelivery, NetRequest};
use rustkit_net::policy::{FetchPolicy, RequestMode, ResponseKind, ScriptRequest, ScriptResponse};
use rustkit_net::{CredentialsMode, RedirectMode, Request, RequestDestination, RequestId, ResourceLoader};
use tracing::debug;
use url::Url;

use crate::inspector::{self, FailureStage, Inspector, ResourceType};

/// What the page sees for every refusal and every failure.
const NETWORK_ERROR: &str = "network error";

/// What one pump found.
#[derive(Debug, Default)]
pub(crate) struct Pump {
    /// The JS engine panicked: its state is unknowable, so the caller runs
    /// nothing more on this page.
    pub poisoned: bool,
    /// Errors the bridge's own JS calls raised (a callback's exception is
    /// caught inside the bridge and lands in the page's error queue).
    pub threw: Vec<String>,
    /// Requests taken and fetched.
    pub requests: usize,
    /// Rounds that did work.
    pub rounds: u32,
    /// What each inserted script did, for the script log.
    pub records: Vec<crate::ScriptRecord>,
}

/// Run take / fetch / deliver rounds until the queue is empty or `rounds`
/// rounds have run. `timers` is the virtual-clock horizon and callback cap
/// to run after each round's deliveries (a response handler may schedule
/// work); `None` delivers only.
///
/// Whatever is still waiting when the rounds or the deadline run out
/// completes with a network error.
pub(crate) async fn pump(
    bindings: &DomBindings,
    policy: &FetchPolicy,
    loader: &ResourceLoader,
    deadline: tokio::time::Instant,
    rounds: u32,
    timers: Option<(u64, u32)>,
    inspector: &Arc<Inspector>,
) -> Pump {
    let mut out = Pump::default();

    for _ in 0..rounds {
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        let Ok(requests) = catch_unwind(AssertUnwindSafe(|| bindings.take_net_requests())) else {
            out.poisoned = true;
            return out;
        };
        if requests.is_empty() {
            return out;
        }
        out.rounds += 1;
        out.requests += requests.len();

        let outcomes = futures::future::join_all(
            requests
                .iter()
                .map(|request| execute(policy, loader, deadline, request, inspector, FailureStage::OverBudget)),
        )
        .await;

        // In the order the page made them (the take order), not completion
        // order.
        for (request, outcome) in requests.iter().zip(outcomes) {
            match catch_unwind(AssertUnwindSafe(|| bindings.deliver_net_response(request.id, outcome))) {
                Ok(Ok(_)) => {}
                Ok(Err(e)) => out.threw.push(e.to_string()),
                Err(_) => {
                    out.poisoned = true;
                    return out;
                }
            }
        }
        if let Some((horizon_ms, max_callbacks)) = timers {
            match catch_unwind(AssertUnwindSafe(|| bindings.run_timers(horizon_ms, max_callbacks))) {
                Ok(Ok(_)) => {}
                Ok(Err(e)) => out.threw.push(e.to_string()),
                Err(_) => {
                    out.poisoned = true;
                    return out;
                }
            }
        }
    }

    // Out of rounds or time. Nothing may be left waiting forever.
    if catch_unwind(AssertUnwindSafe(|| bindings.fail_pending_net_requests(NETWORK_ERROR))).is_err() {
        out.poisoned = true;
    }
    out
}

/// Work the live loop has started and not yet taken the result of. It runs
/// only while [`settle_live`] polls it; dropping it abandons it.
pub(crate) type LiveFuture<T> = Pin<Box<dyn Future<Output = T>>>;

/// A script request that is out: its id and, in the end, its outcome.
pub(crate) type LiveRequest = LiveFuture<(u64, NetDelivery)>;

/// Start `request` for the live loop. Nothing runs until
/// [`settle_live`] polls it; it fails as a network error at `deadline`.
pub(crate) fn start_live(
    policy: Arc<FetchPolicy>,
    loader: Arc<ResourceLoader>,
    request: NetRequest,
    deadline: tokio::time::Instant,
    inspector: Arc<Inspector>,
) -> LiveRequest {
    Box::pin(async move {
        let outcome = execute(&policy, &loader, deadline, &request, &inspector, FailureStage::Timeout).await;
        (request.id, outcome)
    })
}

/// Let what is out make progress for at most `slice`, and take the results
/// that are ready, in the order the work was started. Returns as soon as one
/// is; the rest stays in `in_flight` for a later turn.
///
/// The work lives on the runtime that polls it: every turn of one view must
/// come from the same runtime.
pub(crate) async fn settle_live<T>(in_flight: &mut Vec<LiveFuture<T>>, slice: std::time::Duration) -> Vec<T> {
    let mut done = Vec::new();
    if in_flight.is_empty() {
        return done;
    }
    let pause = tokio::time::sleep(slice);
    tokio::pin!(pause);
    std::future::poll_fn(|cx| {
        let mut i = 0;
        while i < in_flight.len() {
            match in_flight[i].as_mut().poll(cx) {
                Poll::Ready(outcome) => {
                    drop(in_flight.remove(i));
                    done.push(outcome);
                }
                Poll::Pending => i += 1,
            }
        }
        if !done.is_empty() {
            return Poll::Ready(());
        }
        pause.as_mut().poll(cx)
    })
    .await;
    done
}

/// One script request through the policy. The Inspector sees it from here
/// (a request refused before the policy, as malformed, was never made);
/// `on_deadline` is the stage recorded when `deadline` passes first. Dropped
/// before it ends (the page navigated away), it is recorded cancelled.
async fn execute(
    policy: &FetchPolicy,
    loader: &ResourceLoader,
    deadline: tokio::time::Instant,
    request: &NetRequest,
    inspector: &Arc<Inspector>,
    on_deadline: FailureStage,
) -> NetDelivery {
    let script_request = match script_request(request) {
        Ok(r) => r,
        Err(reason) => {
            debug!(id = request.id, %reason, "Script request refused before the policy");
            return NetDelivery::Error(NETWORK_ERROR.into());
        }
    };
    let resource_type = match script_request.destination {
        RequestDestination::Xhr => ResourceType::Xhr,
        RequestDestination::Fetch => ResourceType::Fetch,
        _ => ResourceType::Other,
    };
    let guard = inspector
        .begin_request(&script_request.url, script_request.method.as_str(), resource_type, Some("script"))
        .map(|g| g.dropped_as(FailureStage::Cancelled, "abandoned (navigation or view closed)"));
    match tokio::time::timeout_at(deadline, policy.execute(loader, script_request)).await {
        Ok(Ok(response)) => {
            if let Some(guard) = guard {
                let mime = response.headers.get("content-type").and_then(|v| v.to_str().ok());
                guard.response(response.status, mime, None, Some(response.body.len() as u64), false);
            }
            delivery(response)
        }
        Ok(Err(denial)) => {
            debug!(id = request.id, ?denial, "Script request denied");
            if let Some(guard) = guard {
                let (stage, reason) = inspector::denial_stage(&denial);
                guard.fail(stage, reason);
            }
            NetDelivery::Error(NETWORK_ERROR.into())
        }
        Err(_) => {
            debug!(id = request.id, "Script request ran out of the script budget");
            if let Some(guard) = guard {
                let reason = match on_deadline {
                    FailureStage::OverBudget => "script budget spent",
                    _ => "timed out",
                };
                guard.fail(on_deadline, reason);
            }
            NetDelivery::Error(NETWORK_ERROR.into())
        }
    }
}

/// The bridge's plain data as the policy's request type. Anything malformed
/// is refused here, before the policy; the policy still vets everything else.
fn script_request(request: &NetRequest) -> Result<ScriptRequest, String> {
    let method = Method::from_bytes(request.method.as_bytes()).map_err(|_| "bad method".to_string())?;
    let url = Url::parse(&request.url).map_err(|_| "unparseable url".to_string())?;

    let mut headers = HeaderMap::new();
    for (name, value) in &request.headers {
        let name = HeaderName::from_bytes(name.as_bytes()).map_err(|_| "bad header name".to_string())?;
        let value = HeaderValue::from_str(value).map_err(|_| "bad header value".to_string())?;
        headers.append(name, value);
    }

    let body = match &request.body_b64 {
        Some(b64) => Some(Bytes::from(B64.decode(b64).map_err(|_| "bad body encoding".to_string())?)),
        None => None,
    };

    let mode = match request.mode.as_str() {
        "cors" => RequestMode::Cors,
        "no-cors" => RequestMode::NoCors,
        "same-origin" => RequestMode::SameOrigin,
        _ => return Err("bad mode".into()),
    };
    let credentials = match request.credentials.as_str() {
        "omit" => CredentialsMode::Omit,
        "same-origin" => CredentialsMode::SameOrigin,
        "include" => CredentialsMode::Include,
        _ => return Err("bad credentials".into()),
    };
    let redirect = match request.redirect.as_str() {
        "follow" => RedirectMode::Follow,
        "manual" => RedirectMode::Manual,
        "error" => RedirectMode::Error,
        _ => return Err("bad redirect".into()),
    };
    let destination = match request.destination.as_str() {
        "xhr" => RequestDestination::Xhr,
        "fetch" => RequestDestination::Fetch,
        _ => return Err("bad destination".into()),
    };

    Ok(ScriptRequest {
        id: RequestId::new(),
        method,
        url,
        headers,
        body,
        mode,
        credentials,
        redirect,
        destination,
    })
}

fn delivery(response: ScriptResponse) -> NetDelivery {
    NetDelivery::Response {
        url: response.url.to_string(),
        status: response.status,
        status_text: response.status_text,
        headers: response
            .headers
            .iter()
            .filter_map(|(name, value)| Some((name.as_str().to_string(), value.to_str().ok()?.to_string())))
            .collect(),
        body_b64: B64.encode(&response.body),
        kind: match response.kind {
            ResponseKind::Basic => "basic",
            ResponseKind::Cors => "cors",
            ResponseKind::Opaque => "opaque",
            ResponseKind::OpaqueRedirect => "opaqueredirect",
        }
        .to_string(),
        redirected: response.redirected,
    }
}

/// Most modules one page may pull in. The policy has its own request caps;
/// this bounds the graph walk itself (a page cannot keep the engine here with
/// a generated module graph).
pub(crate) const MAX_MODULES_PER_PAGE: usize = 2_000;

/// Deepest import chain followed (each round fetches one level, all its
/// modules concurrently).
const MAX_MODULE_ROUNDS: u32 = 64;

/// Fetch what a started module graph has asked for, level by level, until it
/// asks for nothing more. Every fetch is the policy's module entry point;
/// this decides nothing. A module that is refused, mistyped, missing or too
/// many is supplied as an error, so the graph fails as a whole and none of
/// its code runs.
///
/// `fetched` counts the modules already pulled in on this page.
pub(crate) async fn pump_modules(
    bindings: &DomBindings,
    policy: &FetchPolicy,
    loader: &ResourceLoader,
    deadline: tokio::time::Instant,
    document: &Url,
    fetched: &mut usize,
    inspector: &Arc<Inspector>,
) -> Pump {
    let mut out = Pump::default();
    for _ in 0..MAX_MODULE_ROUNDS {
        let Ok(urls) = catch_unwind(AssertUnwindSafe(|| bindings.take_module_requests())) else {
            out.poisoned = true;
            return out;
        };
        if urls.is_empty() {
            return out;
        }
        out.rounds += 1;
        out.requests += urls.len();

        // Within the page's module cap, fetch the level concurrently.
        let room = MAX_MODULES_PER_PAGE.saturating_sub(*fetched);
        *fetched += urls.len().min(room);
        let outcomes = futures::future::join_all(urls.iter().enumerate().map(|(i, url)| async move {
            if i >= room {
                return Err(format!("too many modules (limit {MAX_MODULES_PER_PAGE})"));
            }
            let parsed = Url::parse(url).map_err(|_| "unparseable module url".to_string())?;
            let guard = inspector.begin_request(&parsed, "GET", ResourceType::Module, Some("script"));
            let fetched = tokio::time::timeout_at(deadline, policy.fetch_module(loader, &parsed, document)).await;
            inspector::answer_module(guard, &fetched);
            match fetched {
                Ok(Ok(module)) => Ok(rustkit_bindings::FetchedModule {
                    final_url: module.final_url.to_string(),
                    source: module.source,
                }),
                Ok(Err(denial)) => {
                    debug!(%url, ?denial, "Module fetch denied");
                    Err(NETWORK_ERROR.to_string())
                }
                Err(_) => Err("script budget spent".to_string()),
            }
        }))
        .await;

        for (url, outcome) in urls.iter().zip(outcomes) {
            if catch_unwind(AssertUnwindSafe(|| bindings.supply_module(url, outcome))).is_err() {
                out.poisoned = true;
                return out;
            }
        }
    }
    out
}

/// Take-fetch-deliver for the network a page can have waiting: script
/// requests (XHR, fetch) and the modules a dynamic `import()` asked for.
/// One can start the other, so they alternate until none found work, at
/// most `ALTERNATIONS` times. Whatever is still waiting then is left for
/// the next step's pump (the network side fails its stragglers itself).
///
/// Dynamically inserted external scripts are not part of this. They must
/// not delay `DOMContentLoaded` or `load`; [`drive_inserted_scripts`]
/// fetches them and keeps a fetch that outlasts the load's script budget.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn pump_all(
    bindings: &DomBindings,
    policy: &FetchPolicy,
    loader: &ResourceLoader,
    deadline: tokio::time::Instant,
    rounds: u32,
    timers: Option<(u64, u32)>,
    document: Option<&Url>,
    modules_fetched: &mut usize,
    inspector: &Arc<Inspector>,
) -> Pump {
    const ALTERNATIONS: u32 = 4;
    let mut total = Pump::default();
    for _ in 0..ALTERNATIONS {
        let net = pump(bindings, policy, loader, deadline, rounds, timers, inspector).await;
        let modules = match document {
            Some(document) => {
                pump_modules(bindings, policy, loader, deadline, document, modules_fetched, inspector).await
            }
            None => Pump::default(),
        };
        let worked = net.requests + modules.requests;
        total.poisoned |= net.poisoned || modules.poisoned;
        total.threw.extend(net.threw);
        total.threw.extend(modules.threw);
        total.requests += worked;
        total.rounds += net.rounds + modules.rounds;
        if total.poisoned || worked == 0 {
            break;
        }
    }
    total
}

/// Deepest chain of inserted scripts followed in one drive (each round
/// fetches what the previous round's scripts inserted).
const MAX_INSERTED_SCRIPT_ROUNDS: u32 = 16;

/// The window loop's wake. Set by the app; tests leave it empty and call
/// `pump_live` themselves. The callback only wakes the loop. It does not
/// run the script.
static LIVE_WAKE: OnceLock<Mutex<Option<Arc<dyn Fn() + Send + Sync>>>> = OnceLock::new();

/// Install `wake`. A later installed callback replaces the earlier one.
pub(crate) fn set_live_wake(wake: Arc<dyn Fn() + Send + Sync>) {
    let slot = LIVE_WAKE.get_or_init(|| Mutex::new(None));
    *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(wake);
}

fn poke_live_turn() {
    let Some(slot) = LIVE_WAKE.get() else {
        return;
    };
    let wake = slot.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if let Some(wake) = wake {
        wake();
    }
}

/// External classic scripts one view has inserted and not yet run, in
/// insertion order. The fetch runs on the engine's fetch thread, not on
/// the turn that enqueued it. A result is stored here and a live turn runs
/// it. `retire` (a new document) drops whatever is queued and rejects a
/// result stamped with an earlier `nav_id`, so it cannot run on the next
/// page.
pub(crate) struct InsertedScripts {
    board: Arc<Mutex<InsertedBoard>>,
}

struct InsertedBoard {
    /// The document these jobs belong to. A result stamped with another
    /// id is discarded.
    nav_id: u64,
    next_id: u64,
    jobs: Vec<InsertedJob>,
    notify: Arc<tokio::sync::Notify>,
    /// Results stored (and the loop woken) since this board was created.
    wakes: u64,
}

struct InsertedJob {
    id: u64,
    node: usize,
    url: String,
    nav_id: u64,
    body: Option<FetchedInserted>,
    guard: Option<crate::inspector::RequestGuard>,
}

/// What the fetch thread produced. The script itself runs later, inside a
/// turn.
enum FetchedInserted {
    Source(InsertedSource),
    /// A response that was not 2xx. The element hears `error`.
    Http(InsertedHttp),
    Transport {
        stage: FailureStage,
        inspector_reason: &'static str,
        detail: String,
    },
    /// Not fetched (`unparseable src`). No load or error event.
    Skipped(&'static str),
}

struct InsertedSource {
    text: String,
    status: u16,
    mime: Option<String>,
    encoded_size: Option<u64>,
    from_cache: bool,
}

struct InsertedHttp {
    status: u16,
    mime: Option<String>,
    encoded_size: Option<u64>,
    from_cache: bool,
}

struct FetchJob {
    loader: Arc<ResourceLoader>,
    request: Request,
    board: Arc<Mutex<InsertedBoard>>,
    job_id: u64,
    nav_id: u64,
}

impl InsertedScripts {
    pub(crate) fn new() -> Self {
        Self {
            board: Arc::new(Mutex::new(InsertedBoard {
                nav_id: 0,
                next_id: 1,
                jobs: Vec::new(),
                notify: Arc::new(tokio::sync::Notify::new()),
                wakes: 0,
            })),
        }
    }

    /// Drop every queued script and accept only results for `nav_id` from
    /// now on. A fetch already in flight keeps its socket; its result is
    /// discarded when it arrives.
    pub(crate) fn retire(&self, nav_id: u64) {
        let mut board = lock_board(&self.board);
        board.jobs.clear();
        board.nav_id = nav_id;
    }

    /// Drop every queued script without moving the document generation. A
    /// cancelled load uses this: the result must not run later.
    pub(crate) fn abandon(&self) {
        lock_board(&self.board).jobs.clear();
    }

    #[cfg(test)]
    pub(crate) fn wake_count(&self) -> u64 {
        lock_board(&self.board).wakes
    }

    fn has_jobs(&self) -> bool {
        !lock_board(&self.board).jobs.is_empty()
    }

    fn head_ready(&self) -> bool {
        lock_board(&self.board).jobs.first().is_some_and(|job| job.body.is_some())
    }

    fn take_ready_head(&self) -> Option<InsertedJob> {
        let mut board = lock_board(&self.board);
        if board.jobs.first().is_some_and(|job| job.body.is_some()) {
            Some(board.jobs.remove(0))
        } else {
            None
        }
    }

    fn notify(&self) -> Arc<tokio::sync::Notify> {
        lock_board(&self.board).notify.clone()
    }
}

impl Clone for InsertedScripts {
    fn clone(&self) -> Self {
        Self { board: Arc::clone(&self.board) }
    }
}

fn lock_board(board: &Mutex<InsertedBoard>) -> std::sync::MutexGuard<'_, InsertedBoard> {
    board.lock().unwrap_or_else(|e| e.into_inner())
}

/// Fetch and run the external classic scripts the page inserted
/// (`web_scripts.js`), in insertion order. Each is fetched as a
/// parser-inserted script is (the page's referrer, the script destination
/// the shield classifies by, the loader's cookies), never through page
/// script, and run as its element's script: `currentScript` is the element,
/// then the element hears `load`, or `error` when the fetch failed.
///
/// `deadline` bounds how long this call waits. It does not bound the fetch.
/// A fetch still pending when the deadline passes stays queued, hears
/// neither event yet, and is not recorded over budget. The fetch thread
/// keeps it going. When the body arrives it wakes a live turn, which runs
/// the script. `execute` false only enqueues: a turn with no budget left
/// does not run a script. What ran is in the pump's `records`.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn drive_inserted_scripts(
    bindings: &DomBindings,
    loader: Arc<ResourceLoader>,
    referrer: &crate::SubresourceReferrer,
    inspector: &Arc<Inspector>,
    scripts: &InsertedScripts,
    deadline: tokio::time::Instant,
    execute: bool,
) -> Pump {
    let mut out = Pump::default();
    let mut ran = 0u32;
    loop {
        if !enqueue_inserted(bindings, &loader, referrer, inspector, scripts) {
            out.poisoned = true;
            scripts.abandon();
            return out;
        }
        if !scripts.has_jobs() {
            return out;
        }
        if !execute {
            return out;
        }
        if scripts.head_ready() {
            ran += 1;
            out.rounds += 1;
            if run_inserted(bindings, inspector, scripts, &mut out) {
                scripts.abandon();
                return out;
            }
            if ran >= MAX_INSERTED_SCRIPT_ROUNDS {
                // The chain is deeper than one drive follows. Start the next
                // fetch and leave it to the next call; do not drop it.
                if !enqueue_inserted(bindings, &loader, referrer, inspector, scripts) {
                    out.poisoned = true;
                    scripts.abandon();
                }
                return out;
            }
            continue;
        }
        if tokio::time::Instant::now() >= deadline {
            return out;
        }
        let notify = scripts.notify();
        let notified = notify.notified();
        tokio::pin!(notified);
        tokio::select! {
            _ = notified => {}
            _ = tokio::time::sleep_until(deadline) => return out,
        }
    }
}

/// Queue a fetch for every external classic script the page has inserted
/// since the last take, after the ones already queued. `false`: the JS
/// engine panicked taking the queue.
fn enqueue_inserted(
    bindings: &DomBindings,
    loader: &Arc<ResourceLoader>,
    referrer: &crate::SubresourceReferrer,
    inspector: &Arc<Inspector>,
    scripts: &InsertedScripts,
) -> bool {
    let Ok(queued) = catch_unwind(AssertUnwindSafe(|| bindings.take_inserted_scripts())) else {
        return false;
    };
    if queued.is_empty() {
        return true;
    }
    let mut started = Vec::new();
    {
        let mut board = lock_board(&scripts.board);
        for (node, url) in queued {
            let id = board.next_id;
            board.next_id = board.next_id.wrapping_add(1);
            let nav_id = board.nav_id;
            match Url::parse(&url) {
                Ok(parsed) => {
                    let request = referrer.get_for(parsed.clone(), RequestDestination::Script);
                    let guard = inspector.begin_request(&parsed, "GET", ResourceType::Script, Some("script"));
                    board.jobs.push(InsertedJob { id, node, url, nav_id, body: None, guard });
                    started.push(FetchJob {
                        loader: Arc::clone(loader),
                        request,
                        board: Arc::clone(&scripts.board),
                        job_id: id,
                        nav_id,
                    });
                }
                Err(_) => board.jobs.push(InsertedJob {
                    id,
                    node,
                    url,
                    nav_id,
                    body: Some(FetchedInserted::Skipped("unparseable src")),
                    guard: None,
                }),
            }
        }
    }
    for job in started {
        spawn_inserted_fetch(job);
    }
    true
}

/// The fetch thread. One current-thread runtime, so a fetch progresses
/// while the window thread is parked in `ControlFlow::Wait`. The future is
/// built on that thread.
fn spawn_inserted_fetch(job: FetchJob) {
    let tx = fetch_sender();
    let board = Arc::clone(&job.board);
    let job_id = job.job_id;
    let nav_id = job.nav_id;
    if tx.send(job).is_err() {
        debug!("inserted-script fetch thread is gone");
        let failed = FetchedInserted::Transport {
            stage: FailureStage::Connect,
            inspector_reason: "could not connect",
            detail: "fetch thread stopped".into(),
        };
        if store_inserted_result(&board, job_id, nav_id, failed) {
            poke_live_turn();
        }
    }
}

fn fetch_sender() -> &'static tokio::sync::mpsc::UnboundedSender<FetchJob> {
    static TX: OnceLock<tokio::sync::mpsc::UnboundedSender<FetchJob>> = OnceLock::new();
    TX.get_or_init(|| {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        std::thread::Builder::new()
            .name("rk-inserted-script".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("inserted-script fetch runtime");
                let local = tokio::task::LocalSet::new();
                rt.block_on(local.run_until(async move {
                    while let Some(job) = rx.recv().await {
                        tokio::task::spawn_local(run_fetch_job(job));
                    }
                }));
            })
            .expect("inserted-script fetch thread");
        tx
    })
}

async fn run_fetch_job(job: FetchJob) {
    let fetched = fetch_inserted(&job.loader, job.request).await;
    if store_inserted_result(&job.board, job.job_id, job.nav_id, fetched) {
        poke_live_turn();
    }
}

/// `true` when the result was stored for a job that is still the current
/// document's. A navigation or a cancel clears the job first, and this
/// stores nothing.
fn store_inserted_result(board: &Mutex<InsertedBoard>, job_id: u64, nav_id: u64, fetched: FetchedInserted) -> bool {
    let mut board = lock_board(board);
    if board.nav_id != nav_id {
        return false;
    }
    let Some(job) = board.jobs.iter_mut().find(|job| job.id == job_id && job.nav_id == nav_id && job.body.is_none()) else {
        return false;
    };
    job.body = Some(fetched);
    board.wakes += 1;
    let notify = Arc::clone(&board.notify);
    drop(board);
    notify.notify_one();
    true
}

/// One inserted script's HTTP fetch. A 30s stall completes as a network
/// error (the element hears `error` when a turn runs it). This is the
/// request's own timeout, not the turn's budget: the turn stopping its
/// wait does not drop the fetch.
async fn fetch_inserted(loader: &ResourceLoader, request: Request) -> FetchedInserted {
    let deadline = tokio::time::Instant::now() + crate::LIVE_REQUEST_TIMEOUT;
    let response = match tokio::time::timeout_at(deadline, loader.fetch(request)).await {
        Ok(Ok(response)) => response,
        Ok(Err(e)) => {
            let (stage, reason) = inspector::net_error_stage(&e);
            return FetchedInserted::Transport { stage, inspector_reason: reason, detail: e.to_string() };
        }
        Err(_) => {
            return FetchedInserted::Transport {
                stage: FailureStage::Timeout,
                inspector_reason: "timed out",
                detail: "timed out".into(),
            };
        }
    };
    if !response.ok() {
        return FetchedInserted::Http(InsertedHttp {
            status: response.status.as_u16(),
            mime: response_mime(&response),
            encoded_size: response.encoded_body_size,
            from_cache: response.from_cache,
        });
    }
    let http = InsertedHttp {
        status: response.status.as_u16(),
        mime: response_mime(&response),
        encoded_size: response.encoded_body_size,
        from_cache: response.from_cache,
    };
    match tokio::time::timeout_at(deadline, response.text()).await {
        Ok(Ok(text)) => FetchedInserted::Source(InsertedSource {
            text,
            status: http.status,
            mime: http.mime,
            encoded_size: http.encoded_size,
            from_cache: http.from_cache,
        }),
        Ok(Err(e)) => FetchedInserted::Transport {
            stage: FailureStage::Decode,
            inspector_reason: "body is not UTF-8",
            detail: e.to_string(),
        },
        Err(_) => FetchedInserted::Transport {
            stage: FailureStage::Timeout,
            inspector_reason: "timed out",
            detail: "timed out".into(),
        },
    }
}

fn response_mime(response: &rustkit_net::Response) -> Option<String> {
    response
        .content_type
        .as_ref()
        .map(|mime| mime.essence_str().to_string())
        .or_else(|| {
            response
                .headers
                .get(http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string)
        })
}

/// Run the oldest inserted script, whose body has arrived. `true`: the JS
/// engine panicked and nothing further on this page may run.
fn run_inserted(
    bindings: &DomBindings,
    inspector: &Arc<Inspector>,
    scripts: &InsertedScripts,
    out: &mut Pump,
) -> bool {
    use crate::{EngineFailure, FetchFailure, ScriptOutcome, ScriptRecord};
    let Some(mut job) = scripts.take_ready_head() else {
        return false;
    };
    let Some(fetched) = job.body.take() else {
        return false;
    };
    finish_inserted_guard(job.guard.take(), &fetched);
    let mut record = ScriptRecord { source: job.url, bytes: 0, elapsed_ms: 0, outcome: ScriptOutcome::Ran };
    let text = match fetched {
        FetchedInserted::Source(source) if source.text.len() > crate::MAX_PAGE_SCRIPT_BYTES => {
            record.bytes = source.text.len();
            Err(ScriptOutcome::Skipped("too large to run inside the script budget"))
        }
        FetchedInserted::Source(source) => Ok(source.text),
        FetchedInserted::Http(http) => Err(ScriptOutcome::FetchFailed(FetchFailure::Status(http.status))),
        FetchedInserted::Transport { detail, .. } => Err(ScriptOutcome::FetchFailed(FetchFailure::Transport(detail))),
        FetchedInserted::Skipped(reason) => Err(ScriptOutcome::Skipped(reason)),
    };
    let event = match text {
        Err(outcome) => {
            let event = matches!(outcome, ScriptOutcome::FetchFailed(_)).then_some("error");
            record.outcome = outcome;
            event
        }
        Ok(text) => {
            record.bytes = text.len();
            // Named like a parser-inserted external script, not by its URL
            // (which would become its imports' base).
            let name = format!("inserted#{}", job.node);
            inspector.alias_script(&name, &record.source);
            let started = std::time::Instant::now();
            let _ = bindings.set_current_script(Some(job.node));
            let ran = catch_unwind(AssertUnwindSafe(|| bindings.evaluate_named(&text, &name)));
            let _ = bindings.set_current_script(None);
            record.elapsed_ms = started.elapsed().as_millis() as u64;
            record.outcome = match ran {
                Err(_) => ScriptOutcome::EngineFailed(EngineFailure::Panic),
                Ok(Ok(_)) => ScriptOutcome::Ran,
                Ok(Err(e)) => {
                    let message = e.to_string();
                    ScriptOutcome::Threw(
                        message
                            .strip_prefix("JS error: Execution error: ")
                            .map(str::to_string)
                            .unwrap_or(message),
                    )
                }
            };
            if bindings.take_deadline_hit() {
                crate::log_script_stopped(&record.source, record.elapsed_ms, bindings.take_deadline_overrun());
                record.outcome = ScriptOutcome::OverBudget;
            }
            // Run, thrown or not, the element hears `load`.
            matches!(record.outcome, ScriptOutcome::Ran | ScriptOutcome::Threw(_)).then_some("load")
        }
    };
    let poisoned = record.outcome == ScriptOutcome::EngineFailed(EngineFailure::Panic);
    out.records.push(record);
    out.requests += 1;
    if poisoned {
        out.poisoned = true;
        return true;
    }
    if let Some(event) = event {
        if let Err(e) = bindings.fire_script_event(job.node, event) {
            out.threw.push(e.to_string());
        }
    }
    false
}

fn finish_inserted_guard(guard: Option<crate::inspector::RequestGuard>, fetched: &FetchedInserted) {
    let Some(guard) = guard else {
        return;
    };
    match fetched {
        FetchedInserted::Source(source) => guard.response(
            source.status,
            source.mime.as_deref(),
            source.encoded_size,
            Some(source.text.len() as u64),
            source.from_cache,
        ),
        FetchedInserted::Http(http) => {
            guard.response(http.status, http.mime.as_deref(), http.encoded_size, None, http.from_cache);
        }
        FetchedInserted::Transport { stage, inspector_reason, .. } => guard.fail(*stage, inspector_reason),
        FetchedInserted::Skipped(_) => drop(guard),
    }
}
