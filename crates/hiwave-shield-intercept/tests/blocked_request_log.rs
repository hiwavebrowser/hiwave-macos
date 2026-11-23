//! Logs carry shapes, never bodies (the standing rule): the handler logs a blocked request by its
//! host. Its path, query and fragment, and the page it came from, are not logged.
//!
//! In its own test binary on purpose. In the crate's unit tests, other tests call `intercept`
//! on their threads at the same moment; one of them can register `intercept`'s log sites while
//! only the global no-op dispatcher exists and store their interest as "never" just after this
//! test's subscriber was set, so the subscriber saw nothing (tracing-core 0.1.36's single-
//! dispatcher fast path takes no lock; `rebuild_interest_cache()` runs too early to catch it).
//! It failed 26 of 200 runs of the workspace-built unit-test binary on Linux. A separate binary
//! is a separate process: nothing else registers those sites while this test runs.

use std::sync::Arc;

use hiwave_shield_intercept::ShieldInterceptHandler;
use rustkit_net::{InterceptAction, InterceptHandler, Request, RequestDestination, RequestId};
use url::Url;

#[derive(Clone, Default)]
struct Buf(Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for Buf {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Buf {
    type Writer = Buf;
    fn make_writer(&'a self) -> Buf {
        self.clone()
    }
}

fn request(url: &str, destination: RequestDestination, referrer: Option<&str>) -> Request {
    Request {
        id: RequestId::new(),
        url: Url::parse(url).unwrap(),
        method: http::Method::GET,
        headers: Default::default(),
        body: None,
        timeout: None,
        credentials: Default::default(),
        referrer: referrer.map(|r| Url::parse(r).unwrap()),
        referrer_policy: Default::default(),
        destination,
        is_replay_proxied: false,
    }
}

#[test]
fn a_blocked_request_is_logged_by_host_only() {
    let buf = Buf::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(buf.clone())
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .finish();
    let handler = ShieldInterceptHandler::new();
    let action = tracing::subscriber::with_default(subscriber, || {
        handler.intercept(&request(
            "https://adsrvr.org/pixel/u123abc?session=s3cr3t#frag9",
            RequestDestination::Script,
            Some("https://inbox.example/mail/thread42?token=t0k3n"),
        ))
    });
    assert!(!matches!(action, InterceptAction::Allow), "adsrvr.org is blocked: {action:?}");
    let text = String::from_utf8(buf.0.lock().unwrap().clone()).unwrap();
    assert!(text.contains("adsrvr.org"), "the block is logged, by host: {text}");
    for secret in ["pixel", "u123abc", "session", "s3cr3t", "frag9", "inbox", "thread42", "t0k3n"] {
        assert!(!text.contains(secret), "{secret} reached a log line: {text}");
    }
}
