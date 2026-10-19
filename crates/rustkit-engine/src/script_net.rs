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
use rustkit_net::{CredentialsMode, RedirectMode, RequestDestination, RequestId, ResourceLoader};
use tracing::debug;
use url::Url;

use crate::inspector::{self, FailureStage, Inspector, ResourceType};

/// What the page sees for every refusal and every failure.
const NETWORK_ERROR: &str = "network error";

thread_local! {
    /// Answers fetched while the JavaScript budget was already spent. The
    /// load hands them to the next live turn; failing them here would reject
    /// the page's promise, and delivering them would resume it under a
    /// deadline that has already passed.
    static PARKED: std::cell::RefCell<Vec<(u64, NetDelivery)>> = const { std::cell::RefCell::new(Vec::new()) };
}

pub(crate) fn clear_parked_deliveries() {
    PARKED.with(|parked| parked.borrow_mut().clear());
}

pub(crate) fn take_parked_deliveries() -> Vec<(u64, NetDelivery)> {
    PARKED.with(|parked| std::mem::take(&mut *parked.borrow_mut()))
}

fn park_delivery(delivery: (u64, NetDelivery)) {
    PARKED.with(|parked| parked.borrow_mut().push(delivery));
}

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
/// completes with a network error. `deadline` is a network cap: it does
/// not arm the watchdog and it does not refuse a host call. A delivery
/// whose JavaScript budget is already spent is parked for a later turn
/// instead of being run or failed.
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

        let outcomes = futures::future::join_all(requests.iter().map(|request| {
            execute(
                policy,
                loader,
                deadline,
                request,
                inspector,
                FailureStage::Timeout,
            )
        }))
        .await;

        // In the order the page made them (the take order), not completion
        // order. JavaScript budget spent: keep the callback and the answer
        // for a turn that still has some. Delivering now would resume the
        // page under a stop, and the bridge would forget the request.
        for (request, outcome) in requests.iter().zip(outcomes) {
            // Exhausted, or so little left that resuming the page's async
            // function would land in a turn the watchdog stops at once.
            if super::script_js_exhausted() || super::script_js_resume_would_be_stopped() {
                if bindings.shelve_net_request(request.id) {
                    park_delivery((request.id, outcome));
                }
                continue;
            }
            let (delivered, stopped) = super::with_script_js(bindings, || {
                catch_unwind(AssertUnwindSafe(|| {
                    bindings.deliver_net_response(request.id, outcome)
                }))
            });
            match delivered {
                Ok(Ok(_)) => {}
                Ok(Err(e)) => {
                    if !stopped {
                        out.threw.push(e.to_string());
                    }
                }
                Err(_) => {
                    out.poisoned = true;
                    return out;
                }
            }
        }
        if let Some((horizon_ms, max_callbacks)) = timers {
            if !super::script_js_exhausted() {
                let (ran, stopped) = super::with_script_js(bindings, || {
                    catch_unwind(AssertUnwindSafe(|| {
                        bindings.run_timers(horizon_ms, max_callbacks)
                    }))
                });
                match ran {
                    Ok(Ok(_)) => {}
                    Ok(Err(e)) => {
                        if !stopped {
                            out.threw.push(e.to_string());
                        }
                    }
                    Err(_) => {
                        out.poisoned = true;
                        return out;
                    }
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
    match tokio::time::timeout_at(
        deadline,
        super::await_with_chrome_ui(policy.execute(loader, script_request)),
    )
    .await
    {
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
            debug!(id = request.id, "Script request hit its network cap");
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
            let fetched = tokio::time::timeout_at(
                deadline,
                super::await_with_chrome_ui(policy.fetch_module(loader, &parsed, document)),
            )
            .await;
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
                Err(_) => Err("timed out".to_string()),
            }
        }))
        .await;

        for (url, outcome) in urls.iter().zip(outcomes) {
            let (supplied, _) = super::with_script_js(bindings, || {
                catch_unwind(AssertUnwindSafe(|| bindings.supply_module(url, outcome)))
            });
            if supplied.is_err() {
                out.poisoned = true;
                return out;
            }
        }
    }
    out
}

/// Take-fetch-deliver for everything a page can have waiting: script network
/// requests (XHR, fetch), the modules a dynamic `import()` asked for, and the
/// scripts it inserted (with `scripts`, the referrer they are fetched with
/// and the log their outcomes go to). One can start another (a fetch
/// callback calls `import()`, an inserted script fetches), so they alternate
/// until none found work, at most `ALTERNATIONS` times. Whatever is still waiting then is left for the next step's pump
/// (the network side fails its stragglers itself).
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
    scripts: Option<&crate::SubresourceReferrer>,
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
        let inserted = match scripts {
            Some(referrer) => pump_inserted_scripts(bindings, loader, referrer, deadline, inspector).await,
            None => Pump::default(),
        };
        let worked = net.requests + modules.requests + inserted.requests;
        total.poisoned |= net.poisoned || modules.poisoned || inserted.poisoned;
        total.threw.extend(net.threw);
        total.threw.extend(modules.threw);
        total.threw.extend(inserted.threw);
        total.records.extend(inserted.records);
        total.requests += worked;
        total.rounds += net.rounds + modules.rounds + inserted.rounds;
        if total.poisoned || worked == 0 {
            break;
        }
    }
    total
}

/// Deepest chain of inserted scripts followed in one pump (each round
/// fetches what the previous round's scripts inserted).
const MAX_INSERTED_SCRIPT_ROUNDS: u32 = 16;

/// Fetch and run the external classic scripts the page inserted
/// (`web_scripts.js`), round by round, until it inserts no more. Each is
/// fetched as a parser-inserted script is (the page's referrer, the script
/// destination the shield classifies by, the loader's cookies), never
/// through page script, and run as its element's script: `currentScript`
/// is the element, then the element hears `load`, or `error` when the fetch
/// failed. `deadline` is a network cap: a script still out then fails
/// and its element hears `error`. A body that arrived still runs, unless
/// the JavaScript budget is already spent, in which case it is over
/// budget and its element hears neither. What each did is in the pump's
/// `records`.
pub(crate) async fn pump_inserted_scripts(
    bindings: &DomBindings,
    loader: &ResourceLoader,
    referrer: &crate::SubresourceReferrer,
    deadline: tokio::time::Instant,
    inspector: &Arc<Inspector>,
) -> Pump {
    use crate::{EngineFailure, FetchFailure, ScriptOutcome, ScriptRecord};
    let mut out = Pump::default();
    for _ in 0..MAX_INSERTED_SCRIPT_ROUNDS {
        let Ok(scripts) = catch_unwind(AssertUnwindSafe(|| bindings.take_inserted_scripts())) else {
            out.poisoned = true;
            return out;
        };
        if scripts.is_empty() {
            return out;
        }
        out.rounds += 1;
        out.requests += scripts.len();
        let fetched = futures::future::join_all(scripts.iter().map(|(_, url)| async move {
            let url = Url::parse(url).map_err(|_| ScriptOutcome::Skipped("unparseable src"))?;
            let guard = inspector
                .begin_request(&url, "GET", ResourceType::Script, Some("script"))
                .map(|g| g.timeout_at(deadline, "timed out"));
            let fetch = async {
                match super::await_with_chrome_ui(loader.fetch(referrer.get_for(url, RequestDestination::Script))).await {
                    Ok(response) if response.ok() => {
                        let received = guard.map(|g| g.received(&response));
                        match super::await_with_chrome_ui(response.text()).await {
                            Ok(text) => {
                                if let Some(received) = received {
                                    received.read(text.len());
                                }
                                Ok(text)
                            }
                            Err(e) => {
                                if let Some(received) = received {
                                    received.fail(FailureStage::Decode, "body is not UTF-8");
                                }
                                Err(ScriptOutcome::FetchFailed(FetchFailure::Transport(format!("{e}"))))
                            }
                        }
                    }
                    Ok(response) => {
                        if let Some(guard) = guard {
                            guard.received(&response).unread();
                        }
                        Err(ScriptOutcome::FetchFailed(FetchFailure::Status(response.status.as_u16())))
                    }
                    Err(e) => {
                        inspector::fail_net(guard, &e);
                        Err(ScriptOutcome::FetchFailed(FetchFailure::Transport(format!("{e}"))))
                    }
                }
            };
            match tokio::time::timeout_at(deadline, fetch).await {
                Ok(result) => result,
                Err(_) => Err(ScriptOutcome::FetchFailed(FetchFailure::Transport(
                    "timed out".into(),
                ))),
            }
        }))
        .await;

        // Run in the order the page inserted them.
        for ((node, url), result) in scripts.into_iter().zip(fetched) {
            let mut record = ScriptRecord { source: url, bytes: 0, elapsed_ms: 0, outcome: ScriptOutcome::Ran };
            let text = match result {
                Ok(text) if super::script_js_exhausted() => {
                    record.bytes = text.len();
                    Err(ScriptOutcome::OverBudget)
                }
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
                    // Named like a parser-inserted external script, not by
                    // its URL (which would become its imports' base).
                    let name = format!("inserted#{node}");
                    inspector.alias_script(&name, &record.source);
                    let started = std::time::Instant::now();
                    let (ran, stopped) = super::with_script_js(bindings, || {
                        let _ = bindings.set_current_script(Some(node));
                        let ran = catch_unwind(AssertUnwindSafe(|| {
                            bindings.evaluate_named(&text, &name)
                        }));
                        let _ = bindings.set_current_script(None);
                        ran
                    });
                    record.elapsed_ms = started.elapsed().as_millis() as u64;
                    record.outcome = if stopped {
                        crate::log_script_stopped(
                            &record.source,
                            record.elapsed_ms,
                            crate::take_script_js_overrun(),
                        );
                        ScriptOutcome::OverBudget
                    } else {
                        match ran {
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
                        }
                    };                    // Run, thrown or not, the element hears `load`.
                    matches!(record.outcome, ScriptOutcome::Ran | ScriptOutcome::Threw(_)).then_some("load")
                }
            };
            let poisoned = record.outcome == ScriptOutcome::EngineFailed(EngineFailure::Panic);
            out.records.push(record);
            if poisoned {
                out.poisoned = true;
                return out;
            }
            if let Some(event) = event {
                if super::script_js_exhausted() {
                    continue;
                }
                let (fired, stopped) =
                    super::with_script_js(bindings, || bindings.fire_script_event(node, event));
                if stopped {
                    crate::log_script_stopped(
                        &out.records
                            .last()
                            .map(|r| r.source.clone())
                            .unwrap_or_default(),
                        crate::script_js_stopped_ms(),
                        crate::take_script_js_overrun(),
                    );
                    if let Some(record) = out.records.last_mut() {
                        record.outcome = ScriptOutcome::OverBudget;
                    }
                } else if let Err(e) = fired {
                    out.threw.push(e.to_string());
                }
            }
        }
    }
    out
}
