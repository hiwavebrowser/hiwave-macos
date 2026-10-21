//! One HTTP/2 connection per host.
//!
//! Each request used to open its own connection and `tokio::spawn` a driver
//! for it. A page's script fetches start in the same turn as its render-blocking
//! stylesheets, so a host that will only run a few connections (microsoft.com's
//! stylesheets, every bin since the script lists landed) never read the sheet
//! connections. They sat until the client's 30s timeout. Streams on one
//! connection wait on the server's `MAX_CONCURRENT_STREAMS` instead.
//!
//! A shared connection must not depend on any one caller being polled, so
//! the dial, the connection, and each stream's body read all run on
//! [`driver`], a runtime this crate owns. The engine's host runtime is
//! current-thread and only runs while a load or a live turn is driven;
//! a connection spawned there stopped between turns and during script, and
//! every stream on the host waited for the 30s timeout.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::task::{Context, Poll};
use std::time::Duration;

use bytes::Bytes;
use h2::client::SendRequest;
use tokio::sync::{watch, Notify};
use tokio::task::JoinHandle;

#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct H2Key {
    pub host: String,
    pub port: u16,
    /// [`AddressPolicy`](crate::AddressPolicy) tag. A custom policy does not
    /// share a connection with another policy.
    pub policy: u8,
}

pub(crate) const POLICY_ANY: u8 = 0;
pub(crate) const POLICY_PUBLIC: u8 = 1;
pub(crate) const POLICY_CUSTOM: u8 = 2;

/// Streams opened before the peer's SETTINGS arrive. h2's own default is
/// unbounded, so a page's first burst (a hundred scripts, its sheets, its
/// images) went out at once and a server allowing 100 refused the rest.
/// RFC 9113 §6.5.2 recommends a peer allow at least 100.
pub(crate) const DEFAULT_MAX_STREAMS: usize = 100;

/// Receive windows. h2's 64 KiB default lets one or two slow readers hold
/// the whole connection; these are Chrome's.
pub(crate) const STREAM_WINDOW: u32 = 6 * 1024 * 1024;
pub(crate) const CONNECTION_WINDOW: u32 = 15 * 1024 * 1024;

/// The runtime that drives every pooled connection and stream. Built once,
/// never dropped.
pub(crate) fn driver() -> &'static tokio::runtime::Handle {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .thread_name("rk-h2")
                .enable_all()
                .build()
                .expect("the HTTP/2 driver runtime is built once and kept for the process")
        })
        .handle()
}

/// A task on [`driver`] that is aborted when the caller drops it. Dropping
/// the stream sends RST_STREAM(CANCEL) and frees its slot.
pub(crate) struct AbortOnDrop<T>(pub JoinHandle<T>);

impl<T> Drop for AbortOnDrop<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl<T> Future for AbortOnDrop<T> {
    type Output = Result<T, tokio::task::JoinError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.0).poll(cx)
    }
}

/// One HTTP/2 connection and its stream slots.
pub(crate) struct H2Conn {
    pub id: u64,
    pub send: SendRequest<Bytes>,
    open: Mutex<usize>,
    freed: Notify,
}

impl H2Conn {
    pub(crate) fn new(send: SendRequest<Bytes>) -> Arc<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Arc::new(Self {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            send,
            open: Mutex::new(0),
            freed: Notify::new(),
        })
    }

    /// The peer's stream limit, or the builder's initial one until its
    /// SETTINGS arrive.
    pub(crate) fn max_streams(&self) -> usize {
        self.send.current_max_send_streams().max(1)
    }

    /// Wait for a stream slot under the peer's `MAX_CONCURRENT_STREAMS`.
    pub(crate) async fn open_stream(self: &Arc<Self>) -> StreamSlot {
        loop {
            let freed = self.freed.notified();
            tokio::pin!(freed);
            freed.as_mut().enable();
            {
                let mut open = self.open.lock().unwrap_or_else(|e| e.into_inner());
                if *open < self.max_streams() {
                    *open += 1;
                    return StreamSlot(self.clone());
                }
            }
            // SETTINGS can raise the limit without a stream closing.
            let _ = tokio::time::timeout(Duration::from_millis(50), freed).await;
        }
    }
}

/// A held stream slot; dropping it frees the slot.
pub(crate) struct StreamSlot(Arc<H2Conn>);

impl Drop for StreamSlot {
    fn drop(&mut self) {
        let mut open = self.0.open.lock().unwrap_or_else(|e| e.into_inner());
        *open = open.saturating_sub(1);
        drop(open);
        self.0.freed.notify_waiters();
    }
}

/// What a dial to a pooled host produced: an h2 connection now in the
/// pool, or `None` when the host negotiated HTTP/1.1.
pub(crate) type DialOutcome = Result<Option<Arc<H2Conn>>, String>;

enum Slot {
    Dialing(watch::Receiver<Option<DialOutcome>>),
    Ready(Arc<H2Conn>),
}

pub(crate) enum Checkout {
    Ready(Arc<H2Conn>),
    /// Another request is dialing this host; wait for it.
    Wait(watch::Receiver<Option<DialOutcome>>),
    /// Dial, then hand the outcome to [`H2Pool::finish`].
    Lead(DialTicket),
}

pub(crate) struct DialTicket(watch::Sender<Option<DialOutcome>>);

#[derive(Default)]
struct PoolState {
    slots: HashMap<H2Key, Slot>,
    /// The stream limit the host's last connection was given. A new
    /// connection to it starts there instead of at the default, so a retry
    /// burst does not overrun a peer that allows fewer than 100.
    limits: HashMap<H2Key, usize>,
}

/// The lock is never held across an `.await`.
pub(crate) struct H2Pool {
    state: Mutex<PoolState>,
}

impl H2Pool {
    pub(crate) fn new() -> Self {
        Self {
            state: Mutex::new(PoolState::default()),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, PoolState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(crate) fn checkout(&self, key: &H2Key) -> Checkout {
        let mut state = self.state();
        match state.slots.get(key) {
            Some(Slot::Ready(conn)) => return Checkout::Ready(conn.clone()),
            Some(Slot::Dialing(rx)) if rx.has_changed().is_ok() || rx.borrow().is_some() => {
                return Checkout::Wait(rx.clone());
            }
            _ => {}
        }
        let (tx, rx) = watch::channel(None);
        state.slots.insert(key.clone(), Slot::Dialing(rx));
        Checkout::Lead(DialTicket(tx))
    }

    /// Record a dial's outcome and wake the requests waiting on it.
    pub(crate) fn finish(&self, key: &H2Key, ticket: DialTicket, outcome: DialOutcome) {
        {
            let mut state = self.state();
            let ours = matches!(
                state.slots.get(key),
                Some(Slot::Dialing(rx)) if rx.same_channel(&ticket.0.subscribe())
            );
            if ours {
                match &outcome {
                    Ok(Some(conn)) => {
                        state.slots.insert(key.clone(), Slot::Ready(conn.clone()));
                    }
                    _ => {
                        state.slots.remove(key);
                    }
                }
            }
        }
        let _ = ticket.0.send(Some(outcome));
    }

    /// Stop handing out `conn_id` for this host. Its open streams finish.
    pub(crate) fn evict(&self, key: &H2Key, conn_id: u64) {
        let mut state = self.state();
        if matches!(state.slots.get(key), Some(Slot::Ready(conn)) if conn.id == conn_id) {
            if let Some(Slot::Ready(conn)) = state.slots.remove(key) {
                state.limits.insert(key.clone(), conn.max_streams());
            }
        }
    }

    /// Initial stream limit for a new connection to this host.
    pub(crate) fn initial_streams(&self, key: &H2Key) -> usize {
        self.state()
            .limits
            .get(key)
            .copied()
            .unwrap_or(DEFAULT_MAX_STREAMS)
    }
}

impl Default for H2Pool {
    fn default() -> Self {
        Self::new()
    }
}
