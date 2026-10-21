//! The shared HTTP/2 connection against a local h2c server.
//!
//! The server limits streams to what a test sets, refuses or never answers
//! a stream on request, and (in its raw form) sends a GOAWAY mid-burst.
//! It runs on its own runtime thread so a test can park the client's
//! runtime without parking the server.

use super::*;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex as StdMutex;
use std::time::{Duration, Instant};

use h2::Reason;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// What the server saw.
#[derive(Default)]
struct Seen {
    accepts: AtomicUsize,
    /// `"GET /a"` → streams opened for it, refused ones included.
    hits: StdMutex<HashMap<String, usize>>,
}

impl Seen {
    fn hits(&self, key: &str) -> usize {
        self.hits.lock().unwrap().get(key).copied().unwrap_or(0)
    }
}

const BIG_FIRST: usize = 16 * 1024;
const BIG_REST: usize = 512 * 1024;

/// Paths: `/hang` is never answered. `/refuse-once` is refused
/// (RST_STREAM REFUSED_STREAM) the first time and answered after. `/big`
/// sends 16 KiB, waits 150 ms, then sends 512 KiB more. Anything else is
/// `ok`.
async fn serve_h2(sock: tokio::net::TcpStream, max_streams: u32, seen: Arc<Seen>) {
    let mut connection = match h2::server::Builder::new()
        .max_concurrent_streams(max_streams)
        .handshake::<_, Bytes>(sock)
        .await
    {
        Ok(connection) => connection,
        Err(_) => return,
    };
    while let Some(next) = connection.accept().await {
        let Ok((request, mut respond)) = next else {
            break;
        };
        let key = format!("{} {}", request.method(), request.uri().path());
        let count = {
            let mut hits = seen.hits.lock().unwrap();
            let n = hits.entry(key).or_default();
            *n += 1;
            *n
        };
        let path = request.uri().path().to_string();
        tokio::spawn(async move {
            match path.as_str() {
                "/hang" => {
                    let _held = respond;
                    std::future::pending::<()>().await;
                }
                "/refuse-once" if count == 1 => respond.send_reset(Reason::REFUSED_STREAM),
                "/big" => {
                    let head = http::Response::builder().status(200).body(()).unwrap();
                    let Ok(mut body) = respond.send_response(head, false) else {
                        return;
                    };
                    let _ = body.send_data(Bytes::from(vec![b'a'; BIG_FIRST]), false);
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    let _ = body.send_data(Bytes::from(vec![b'b'; BIG_REST]), true);
                    // Keep the stream until the client reads it.
                    let _ = std::future::poll_fn(|cx| body.poll_reset(cx)).await;
                }
                _ => {
                    let head = http::Response::builder().status(200).body(()).unwrap();
                    if let Ok(mut body) = respond.send_response(head, false) {
                        let _ = body.send_data(Bytes::from_static(b"ok"), true);
                    }
                }
            }
        });
    }
}

/// A server thread with its own runtime. Returns its address.
/// `settings_delay` holds the server's preface back, so a client burst goes
/// out before the peer's SETTINGS, as it does on a real network.
fn spawn_server(
    max_streams: u32,
    settings_delay: Duration,
    raw_first: Option<RawGoAway>,
) -> (SocketAddr, Arc<Seen>) {
    let seen = Arc::new(Seen::default());
    let (tx, rx) = std::sync::mpsc::channel();
    let thread_seen = seen.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async move {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            tx.send(listener.local_addr().unwrap()).unwrap();
            let mut raw_first = raw_first;
            loop {
                let Ok((sock, _)) = listener.accept().await else {
                    break;
                };
                thread_seen.accepts.fetch_add(1, Ordering::SeqCst);
                let seen = thread_seen.clone();
                match raw_first.take() {
                    Some(raw) => {
                        tokio::spawn(raw.serve(sock, seen));
                    }
                    None => {
                        tokio::spawn(async move {
                            tokio::time::sleep(settings_delay).await;
                            serve_h2(sock, max_streams, seen).await
                        });
                    }
                }
            }
        });
    });
    (rx.recv().unwrap(), seen)
}

/// A first connection served by hand: read `burst` request HEADERS,
/// answer the first `answer` of them, then GOAWAY(last answered,
/// PROTOCOL_ERROR) and close. The h2 crate's server cannot send a GOAWAY
/// whose last stream id is below a stream it has already read.
struct RawGoAway {
    burst: usize,
    answer: usize,
}

impl RawGoAway {
    async fn serve(self, mut sock: tokio::net::TcpStream, seen: Arc<Seen>) {
        fn frame(kind: u8, flags: u8, stream: u32, payload: &[u8]) -> Vec<u8> {
            let len = payload.len() as u32;
            let mut out = vec![(len >> 16) as u8, (len >> 8) as u8, len as u8, kind, flags];
            out.extend_from_slice(&stream.to_be_bytes());
            out.extend_from_slice(payload);
            out
        }
        let mut preface = [0u8; 24];
        if sock.read_exact(&mut preface).await.is_err() {
            return;
        }
        let _ = sock.write_all(&frame(0x4, 0, 0, &[])).await;
        let mut streams = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        while streams.len() < self.burst {
            let mut head = [0u8; 9];
            match tokio::time::timeout_at(deadline, sock.read_exact(&mut head)).await {
                Ok(Ok(_)) => {}
                _ => break,
            }
            let len = (head[0] as usize) << 16 | (head[1] as usize) << 8 | head[2] as usize;
            let (kind, flags) = (head[3], head[4]);
            let stream = u32::from_be_bytes([head[5], head[6], head[7], head[8]]) & 0x7fff_ffff;
            let mut payload = vec![0u8; len];
            if sock.read_exact(&mut payload).await.is_err() {
                return;
            }
            match kind {
                0x4 if flags & 0x1 == 0 => {
                    let _ = sock.write_all(&frame(0x4, 0x1, 0, &[])).await;
                }
                0x1 => streams.push(stream),
                _ => {}
            }
        }
        streams.sort_unstable();
        *seen.hits.lock().unwrap().entry("raw".into()).or_default() += streams.len();
        let answered = &streams[..self.answer.min(streams.len())];
        let mut out = Vec::new();
        for &id in answered {
            // HPACK static index 8: `:status: 200`.
            out.extend(frame(0x1, 0x4, id, &[0x88]));
            out.extend(frame(0x0, 0x1, id, b"ok"));
        }
        let last = answered.last().copied().unwrap_or(0);
        let mut goaway = last.to_be_bytes().to_vec();
        goaway.extend_from_slice(&1u32.to_be_bytes());
        out.extend(frame(0x7, 0, 0, &goaway));
        let _ = sock.write_all(&out).await;
        let _ = sock.flush().await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        let _ = sock.shutdown().await;
    }
}

fn bounded<T>(
    fut: impl std::future::Future<Output = Result<T, HttpError>>,
) -> impl std::future::Future<Output = Result<T, HttpError>> {
    async move {
        tokio::time::timeout(Duration::from_secs(5), fut)
            .await
            .unwrap_or(Err(HttpError::Timeout))
    }
}

/// The server allows two streams. Ten GETs go out in one burst before its
/// SETTINGS can arrive. Every one must be answered: the client queues
/// streams past the peer's limit instead of having them refused.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ten_concurrent_gets_past_a_two_stream_limit_all_succeed() {
    let (addr, seen) = spawn_server(2, Duration::from_millis(100), None);
    let client = Client::new().unwrap();
    let results = futures::future::join_all((0..10).map(|i| {
        let client = client.clone();
        async move { bounded(client.request_h2_cleartext(&format!("http://{addr}/s{i}"))).await }
    }))
    .await;
    for (i, result) in results.into_iter().enumerate() {
        let raw = result.unwrap_or_else(|e| panic!("GET /s{i}: {e}"));
        assert_eq!(raw.body.as_ref(), b"ok");
    }
    for i in 0..10 {
        assert_eq!(seen.hits(&format!("GET /s{i}")), 1, "/s{i} was sent more than once");
    }
}

/// RFC 9113 §8.7: a refused stream was not processed, so a GET may be
/// retried. Once, on a new connection.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refused_get_is_retried_once_on_a_fresh_connection() {
    let (addr, seen) = spawn_server(100, Duration::ZERO, None);
    let client = Client::new().unwrap();
    let raw = bounded(client.request_h2_cleartext(&format!("http://{addr}/refuse-once")))
        .await
        .expect("the refused GET is retried");
    assert_eq!(raw.body.as_ref(), b"ok");
    assert_eq!(seen.hits("GET /refuse-once"), 2, "exactly one retry");
    assert_eq!(seen.accepts.load(Ordering::SeqCst), 2, "the retry uses a fresh connection");
}

/// A POST is not idempotent. Refused or not, it is not sent twice.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refused_post_is_not_retried() {
    let (addr, seen) = spawn_server(100, Duration::ZERO, None);
    let client = Client::new().unwrap();
    let result = bounded(client.request_h2c(
        Method::POST,
        &format!("http://{addr}/refuse-once"),
        Some(Bytes::from_static(b"form=1")),
    ))
    .await;
    assert!(result.is_err(), "the refused POST must surface, not be resent");
    assert_eq!(seen.hits("POST /refuse-once"), 1);
}

/// GOAWAY(last_stream_id=3) after six streams: streams 1 and 3 were
/// answered, 5..11 were never processed. The four unprocessed GETs go out
/// again on a new connection; the two answered ones do not.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_goaway_retries_unprocessed_gets_and_not_processed_ones() {
    let (addr, seen) = spawn_server(100, Duration::ZERO, Some(RawGoAway { burst: 6, answer: 2 }));
    let client = Client::new().unwrap();
    let results = futures::future::join_all((0..6).map(|i| {
        let client = client.clone();
        async move { bounded(client.request_h2_cleartext(&format!("http://{addr}/g{i}"))).await }
    }))
    .await;
    for (i, result) in results.into_iter().enumerate() {
        let raw = result.unwrap_or_else(|e| panic!("GET /g{i}: {e}"));
        assert_eq!(raw.body.as_ref(), b"ok");
    }
    assert_eq!(seen.hits("raw"), 6, "all six went out on the first connection");
    let resent: usize = (0..6).map(|i| seen.hits(&format!("GET /g{i}"))).sum();
    assert_eq!(resent, 4, "only the four unprocessed streams are retried");
    assert_eq!(seen.accepts.load(Ordering::SeqCst), 2);
}

/// The app path: a request starts on the host runtime, which then stops
/// being driven (the load returned, or script is running). Nine requests
/// to the same host from another runtime must not wait on it. Before,
/// the connection driver was spawned on the runtime that dialed, so it
/// stopped with that runtime and every stream on the host hung until the
/// 30 s timeout.
#[test]
fn a_request_parked_on_an_idle_runtime_does_not_hold_the_host() {
    let (addr, _seen) = spawn_server(2, Duration::ZERO, None);
    let client = Client::new().unwrap();
    let host_rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let hang_client = client.clone();
    let mut hung =
        Box::pin(async move { hang_client.request_h2_cleartext(&format!("http://{addr}/hang")).await });
    host_rt.block_on(async {
        tokio::select! {
            _ = &mut hung => panic!("/hang was answered"),
            _ = tokio::time::sleep(Duration::from_millis(100)) => {}
        }
    });

    let other_rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let timings = other_rt.block_on(futures::future::join_all((0..9).map(|i| {
        let client = client.clone();
        async move {
            let started = Instant::now();
            let result = bounded(client.request_h2_cleartext(&format!("http://{addr}/n{i}"))).await;
            (i, result, started.elapsed())
        }
    })));
    for (i, result, took) in timings {
        let raw = result.unwrap_or_else(|e| panic!("GET /n{i} behind the hung stream: {e}"));
        assert_eq!(raw.body.as_ref(), b"ok");
        assert!(
            took < Duration::from_millis(300),
            "GET /n{i} waited {took:?} behind the hung stream"
        );
    }
    drop(hung);
}

/// A response whose body is not being read (an image future parked on a
/// yielding load) must not hold the connection's flow-control window.
/// Before, a body was read only while its caller polled it. Two parked
/// bodies hold the whole 64 KiB connection window, and every other stream
/// on the host stalls.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_parked_response_body_does_not_stall_the_connection() {
    let (addr, _seen) = spawn_server(100, Duration::ZERO, None);
    let client = Client::new().unwrap();
    let mut parked: Vec<_> = (0..2)
        .map(|_| {
            let client = client.clone();
            Box::pin(async move { client.request_h2_cleartext(&format!("http://{addr}/big")).await })
        })
        .collect();
    {
        let mut both = futures::future::join_all(parked.iter_mut());
        tokio::select! {
            _ = &mut both => panic!("/big finished before its second half was sent"),
            _ = tokio::time::sleep(Duration::from_millis(80)) => {}
        }
    }
    // Both are parked now. Their second halves arrive while nobody polls them.
    tokio::time::sleep(Duration::from_millis(200)).await;
    let timings = futures::future::join_all((0..9).map(|i| {
        let client = client.clone();
        async move {
            let started = Instant::now();
            let result = bounded(client.request_h2_cleartext(&format!("http://{addr}/p{i}"))).await;
            (i, result, started.elapsed())
        }
    }))
    .await;
    for (i, result, took) in timings {
        let raw = result.unwrap_or_else(|e| panic!("GET /p{i} behind the parked bodies: {e}"));
        assert_eq!(raw.body.as_ref(), b"ok");
        assert!(
            took < Duration::from_millis(300),
            "GET /p{i} waited {took:?} behind the parked bodies"
        );
    }
    for big in parked {
        let raw = bounded(big).await.expect("/big still completes");
        assert_eq!(raw.body.len(), BIG_FIRST + BIG_REST);
    }
}
