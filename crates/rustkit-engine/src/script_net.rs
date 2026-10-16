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
use std::sync::Arc;
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

/// An external classic script the page inserted, in insertion order, whose
/// body is still arriving or has arrived and not yet run.
///
/// The fetch keeps running while this value lives. Dropping it (a
/// navigation, a closed view) abandons the request. A script budget that
/// runs out does not drop it: the load parks the vec and the live loop
/// polls it again.
pub(crate) struct PendingInserted {
    node: usize,
    url: String,
    /// The fetch, until it completes. `None` once `ready` holds the result
    /// (a later script can finish first; this one stays so order is kept).
    fetch: Option<LiveFuture<Result<String, crate::ScriptOutcome>>>,
    ready: Option<Result<String, crate::ScriptOutcome>>,
}

/// Fetch and run the external classic scripts the page inserted
/// (`web_scripts.js`), in insertion order. Each is fetched as a
/// parser-inserted script is (the page's referrer, the script destination
/// the shield classifies by, the loader's cookies), never through page
/// script, and run as its element's script: `currentScript` is the element,
/// then the element hears `load`, or `error` when the fetch failed.
///
/// `deadline` bounds how long this call waits. A fetch still pending then
/// stays in `pending` and hears neither event yet; nothing is recorded over
/// budget and nothing is dropped. `execute` false polls the fetches (so a
/// download keeps moving) without running a script. What ran is in the
/// pump's `records`.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn drive_inserted_scripts(
    bindings: &DomBindings,
    loader: Arc<ResourceLoader>,
    referrer: &crate::SubresourceReferrer,
    inspector: &Arc<Inspector>,
    pending: &mut Vec<PendingInserted>,
    deadline: tokio::time::Instant,
    execute: bool,
) -> Pump {
    let mut out = Pump::default();
    for _ in 0..MAX_INSERTED_SCRIPT_ROUNDS {
        if !enqueue_inserted(bindings, &loader, referrer, inspector, pending) {
            out.poisoned = true;
            pending.clear();
            return out;
        }
        if pending.is_empty() {
            return out;
        }
        if !execute {
            let _ = head_is_ready(pending, deadline).await;
            return out;
        }
        if !head_is_ready(pending, deadline).await {
            return out;
        }
        out.rounds += 1;
        if run_inserted(bindings, inspector, pending, &mut out) {
            pending.clear();
            return out;
        }
    }
    // The chain is deeper than one drive follows. Start the next fetch and
    // leave it to the next call; do not drop it.
    if !enqueue_inserted(bindings, &loader, referrer, inspector, pending) {
        out.poisoned = true;
        pending.clear();
        return out;
    }
    if !pending.is_empty() {
        let _ = head_is_ready(pending, tokio::time::Instant::now()).await;
    }
    out
}

/// Queue a fetch for every external classic script the page has inserted
/// since the last take, after the ones already in `pending`. `false`: the
/// JS engine panicked taking the queue.
fn enqueue_inserted(
    bindings: &DomBindings,
    loader: &Arc<ResourceLoader>,
    referrer: &crate::SubresourceReferrer,
    inspector: &Arc<Inspector>,
    pending: &mut Vec<PendingInserted>,
) -> bool {
    use crate::ScriptOutcome;
    let Ok(scripts) = catch_unwind(AssertUnwindSafe(|| bindings.take_inserted_scripts())) else {
        return false;
    };
    for (node, url) in scripts {
        match Url::parse(&url) {
            Ok(parsed) => {
                let request = referrer.get_for(parsed.clone(), RequestDestination::Script);
                pending.push(PendingInserted {
                    node,
                    url,
                    fetch: Some(start_inserted_fetch(loader.clone(), inspector.clone(), parsed, request)),
                    ready: None,
                });
            }
            Err(_) => pending.push(PendingInserted {
                node,
                url,
                fetch: None,
                ready: Some(Err(ScriptOutcome::Skipped("unparseable src"))),
            }),
        }
    }
    true
}

/// The fetch of one inserted script. A 30s stall completes as a network
/// error (the element hears `error` when the script then runs). Dropping
/// the future abandons the request. It is not tied to the load's script
/// budget: that deadline only stops the caller from waiting.
fn start_inserted_fetch(
    loader: Arc<ResourceLoader>,
    inspector: Arc<Inspector>,
    url: Url,
    request: Request,
) -> LiveFuture<Result<String, crate::ScriptOutcome>> {
    Box::pin(async move {
        use crate::{FetchFailure, ScriptOutcome};
        let guard = inspector.begin_request(&url, "GET", ResourceType::Script, Some("script"));
        let deadline = tokio::time::Instant::now() + crate::LIVE_REQUEST_TIMEOUT;
        let response = match tokio::time::timeout_at(deadline, loader.fetch(request)).await {
            Ok(Ok(response)) => response,
            Ok(Err(e)) => {
                inspector::fail_net(guard, &e);
                return Err(ScriptOutcome::FetchFailed(FetchFailure::Transport(format!("{e}"))));
            }
            Err(_) => {
                if let Some(guard) = guard {
                    guard.fail(FailureStage::Timeout, "timed out");
                }
                return Err(ScriptOutcome::FetchFailed(FetchFailure::Transport("timed out".into())));
            }
        };
        if !response.ok() {
            if let Some(guard) = guard {
                guard.received(&response).unread();
            }
            return Err(ScriptOutcome::FetchFailed(FetchFailure::Status(response.status.as_u16())));
        }
        let received = guard.map(|g| g.received(&response));
        match tokio::time::timeout_at(deadline, response.text()).await {
            Ok(Ok(text)) => {
                if let Some(received) = received {
                    received.read(text.len());
                }
                Ok(text)
            }
            Ok(Err(e)) => {
                if let Some(received) = received {
                    received.fail(FailureStage::Decode, "body is not UTF-8");
                }
                Err(ScriptOutcome::FetchFailed(FetchFailure::Transport(format!("{e}"))))
            }
            Err(_) => {
                if let Some(received) = received {
                    received.fail(FailureStage::Timeout, "timed out");
                }
                Err(ScriptOutcome::FetchFailed(FetchFailure::Transport("timed out".into())))
            }
        }
    })
}

/// Poll every in-flight fetch so a slow one keeps downloading, and report
/// whether the oldest script can run. A fetch that finishes out of order
/// keeps its body in `ready`; only the head runs. One poll happens even
/// when `deadline` has already passed, so the request is sent.
async fn head_is_ready(pending: &mut [PendingInserted], deadline: tokio::time::Instant) -> bool {
    if pending.is_empty() {
        return false;
    }
    let pause = tokio::time::sleep_until(deadline);
    tokio::pin!(pause);
    std::future::poll_fn(|cx| {
        settle_ready(pending, cx);
        if pending[0].ready.is_some() {
            return Poll::Ready(true);
        }
        if tokio::time::Instant::now() >= deadline {
            return Poll::Ready(false);
        }
        pause.as_mut().poll(cx).map(|_| false)
    })
    .await
}

fn settle_ready(pending: &mut [PendingInserted], cx: &mut std::task::Context<'_>) {
    for slot in pending.iter_mut() {
        if slot.ready.is_some() {
            continue;
        }
        let polled = slot.fetch.as_mut().map(|fetch| fetch.as_mut().poll(cx));
        if let Some(Poll::Ready(result)) = polled {
            slot.fetch = None;
            slot.ready = Some(result);
        }
    }
}

/// Run the oldest inserted script, which [`head_is_ready`] has finished.
/// `true`: the JS engine panicked and `pending` must not run anything more.
fn run_inserted(
    bindings: &DomBindings,
    inspector: &Arc<Inspector>,
    pending: &mut Vec<PendingInserted>,
    out: &mut Pump,
) -> bool {
    use crate::{EngineFailure, ScriptOutcome, ScriptRecord};
    let Some(slot) = pending.first() else {
        return false;
    };
    if slot.ready.is_none() {
        return false;
    }
    let slot = pending.remove(0);
    let Some(result) = slot.ready else {
        return false;
    };
    let mut record = ScriptRecord { source: slot.url, bytes: 0, elapsed_ms: 0, outcome: ScriptOutcome::Ran };
    let text = match result {
        Ok(text) if text.len() > crate::MAX_PAGE_SCRIPT_BYTES => {
            record.bytes = text.len();
            Err(ScriptOutcome::Skipped("too large to run inside the script budget"))
        }
        other => other,
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
            let name = format!("inserted#{}", slot.node);
            inspector.alias_script(&name, &record.source);
            let started = std::time::Instant::now();
            let _ = bindings.set_current_script(Some(slot.node));
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
        if let Err(e) = bindings.fire_script_event(slot.node, event) {
            out.threw.push(e.to_string());
        }
    }
    false
}
