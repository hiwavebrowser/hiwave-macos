//! Process-lifetime current-thread runtime for page loads and live turns.
//!
//! A navigation used to build a runtime, `block_on` the load, and drop it.
//! Image fetches started on that runtime carry `tokio::time::timeout`
//! sleeps bound to its driver. The next live turn polled those futures on
//! a second runtime after the first had shut down. Tokio 1.49 panics in
//! `Sleep::poll_elapsed` when the driver is shutting down
//! ("A Tokio 1.x context was found, but it is being shutdown"). Dropping a
//! `Sleep` does not panic; polling it after the driver is gone does.
//!
//! One runtime is built on first use and never dropped, so a future or a
//! timer polled on a later turn still has its driver. `drive` serializes
//! `block_on`: a current-thread runtime has one core, and `load_url`'s
//! future is `!Send`, so this cannot be a multi-thread runtime.
//!
//! Parser image fetches are that future. They are polled during the script
//! walk and, when [`Engine::load_url_yielding`](crate::Engine::load_url_yielding)
//! returns at `DOMContentLoaded`, parked on the view until a later live
//! turn awaits them. They are not spawned onto another runtime. An HTTP/2
//! connection driver `tokio::spawn`ed from inside one is on this runtime
//! too, so returning from the load does not abort the connection.
//!
//! `shutdown_background` is how a runtime is dropped from inside itself
//! without polling its tasks on the way down. Nothing here drops the
//! runtime, so that call is not used. `spawn_blocking` is not used either:
//! image decode stays on the thread that drives the turn. The panic was
//! the time driver, not a blocking pool.

use std::cell::Cell;
use std::future::Future;
use std::sync::{Mutex, OnceLock};

use tokio::runtime::Runtime;

fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .thread_name("rk-host")
            .build()
            .expect("the host runtime is built once and kept for the process")
    })
}

fn gate() -> &'static Mutex<()> {
    static GATE: OnceLock<Mutex<()>> = OnceLock::new();
    GATE.get_or_init(|| Mutex::new(()))
}

thread_local! {
    static DRIVING: Cell<bool> = const { Cell::new(false) };
}

struct DrivingGuard;

impl Drop for DrivingGuard {
    fn drop(&mut self) {
        DRIVING.with(|driving| driving.set(false));
    }
}

/// Drive `fut` on the process host runtime.
///
/// Concurrent callers wait. A second call on the thread already inside
/// `drive` panics: the current-thread runtime cannot nest `block_on`, and
/// the load already holds the engine.
pub fn drive<F>(fut: F) -> F::Output
where
    F: Future,
{
    if DRIVING.with(|driving| driving.replace(true)) {
        panic!("host runtime drive re-entered on this thread");
    }
    let _driving = DrivingGuard;
    let _turn = gate()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    runtime().block_on(fut)
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::time::Duration;

    use super::drive;

    /// Drop the page while an image fetch and a timer are in flight, then
    /// drive another timer. Both turns use [`drive`]. A runtime built for
    /// the first turn and dropped before the second panics in Tokio's time
    /// driver when the same futures are polled again.
    #[test]
    fn replacing_a_page_drops_in_flight_image_fetches_and_timers() {
        use std::io::{ErrorKind, Read};
        use std::net::TcpListener;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::{mpsc, Arc};
        use std::thread;

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let seen = Arc::new(AtomicBool::new(false));
        let seen_thread = Arc::clone(&seen);
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        thread::spawn(move || {
            let started = std::time::Instant::now();
            loop {
                if stop_rx.try_recv().is_ok() || started.elapsed() > Duration::from_secs(8) {
                    break;
                }
                match listener.accept() {
                    Ok((mut sock, _)) => {
                        let _ = sock.set_read_timeout(Some(Duration::from_millis(500)));
                        let mut buf = [0u8; 2048];
                        let _ = sock.read(&mut buf);
                        seen_thread.store(true, Ordering::SeqCst);
                        while stop_rx.try_recv().is_err() {
                            thread::sleep(Duration::from_millis(20));
                        }
                        break;
                    }
                    Err(err) if err.kind() == ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });

        let mut engine =
            super::super::Engine::new(super::super::EngineConfig::default()).expect("engine");
        let view = engine
            .create_headless_view(rustkit_viewhost::Bounds {
                x: 0,
                y: 0,
                width: 100,
                height: 100,
            })
            .expect("view");
        engine
            .load_html(
                view,
                &format!(
                    r#"<html><body><img src="http://127.0.0.1:{port}/hang.png"></body></html>"#
                ),
            )
            .expect("html");
        // about:blank may not fetch loopback. The page is this server, so
        // the subresource vet allows the image.
        engine.views.get_mut(&view).unwrap().url =
            Some(url::Url::parse(&format!("http://127.0.0.1:{port}/page")).unwrap());

        let mut timer = drive(async {
            let plan = engine
                .plan_image_loads(view, false)
                .expect("plan")
                .expect("document");
            assert!(!plan.is_empty(), "the hanging image is fetched");
            engine
                .views
                .get_mut(&view)
                .unwrap()
                .live_images
                .push(Box::pin(super::super::Engine::fetch_planned_images(plan)));
            let started = std::time::Instant::now();
            while !seen.load(Ordering::SeqCst) && started.elapsed() < Duration::from_secs(2) {
                let view_state = engine.views.get_mut(&view).unwrap();
                let _ = super::super::script_net::settle_live(
                    &mut view_state.live_images,
                    Duration::from_millis(30),
                )
                .await;
            }
            assert!(
                seen.load(Ordering::SeqCst),
                "the image request reached the server"
            );
            assert_eq!(
                engine.views.get(&view).unwrap().live_images.len(),
                1,
                "the fetch is still in flight"
            );
            let mut timer = Box::pin(tokio::time::sleep(Duration::from_millis(40)));
            let waker = futures::task::noop_waker();
            let mut cx = std::task::Context::from_waker(&waker);
            assert!(
                std::pin::Pin::new(&mut timer).poll(&mut cx).is_pending(),
                "the timer is registered on this runtime"
            );
            timer
        });

        drive(async {
            tokio::time::timeout(Duration::from_secs(2), &mut timer)
                .await
                .expect("the in-flight timer still runs");
            {
                let view_state = engine.views.get_mut(&view).unwrap();
                let _ = super::super::script_net::settle_live(
                    &mut view_state.live_images,
                    Duration::from_millis(20),
                )
                .await;
            }
            assert_eq!(
                engine.views.get(&view).unwrap().live_images.len(),
                1,
                "the second turn polled the same image future"
            );
            engine
                .load_html(view, "<html><body>replaced</body></html>")
                .expect("replace");
            assert!(
                engine.views.get(&view).unwrap().live_images.is_empty(),
                "replacing the page drops the in-flight image fetch"
            );
            tokio::time::timeout(
                Duration::from_secs(2),
                tokio::time::sleep(Duration::from_millis(20)),
            )
            .await
            .expect("a timer after the page is replaced");
        });

        let _ = stop_tx.send(());
    }

    #[test]
    fn a_timer_started_on_one_turn_runs_on_the_next() {
        let mut timer = drive(async {
            let mut timer = Box::pin(tokio::time::sleep(Duration::from_millis(40)));
            let waker = futures::task::noop_waker();
            let mut cx = std::task::Context::from_waker(&waker);
            assert!(
                std::pin::Pin::new(&mut timer).poll(&mut cx).is_pending(),
                "the timer is registered and still in flight"
            );
            timer
        });
        drive(async {
            tokio::time::timeout(Duration::from_secs(2), &mut timer)
                .await
                .expect("the same runtime still drives the timer");
        });
    }

    /// A yielding load returns at DOMContentLoaded while a parser image
    /// fetch is still in flight. The bytes arrive afterwards, on a later
    /// [`drive`]. Dropping the load's runtime at the return used to kill
    /// that socket (Wikipedia: "No route to host"; an h2 page: broken pipe).
    #[test]
    fn an_image_fetch_started_before_dom_content_loaded_finishes_after_the_load_returns() {
        use std::io::{Read, Write};
        use std::net::{TcpListener, TcpStream};
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        use std::thread;

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let seen = Arc::new(AtomicBool::new(false));
        let release = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let seen_thread = Arc::clone(&seen);
        let release_thread = Arc::clone(&release);
        let stop_thread = Arc::clone(&stop);
        thread::spawn(move || {
            let started = std::time::Instant::now();
            while !stop_thread.load(Ordering::SeqCst) && started.elapsed() < Duration::from_secs(20)
            {
                match listener.accept() {
                    Ok((sock, _)) => {
                        let seen = Arc::clone(&seen_thread);
                        let release = Arc::clone(&release_thread);
                        thread::spawn(move || answer(sock, &seen, &release));
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });

        fn answer(mut sock: TcpStream, seen: &AtomicBool, release: &AtomicBool) {
            let _ = sock.set_read_timeout(Some(Duration::from_secs(3)));
            let mut buf = Vec::new();
            let mut tmp = [0u8; 1024];
            while !buf.windows(4).any(|w| w == b"\r\n\r\n") && buf.len() < 8192 {
                match sock.read(&mut tmp) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => buf.extend_from_slice(&tmp[..n]),
                }
            }
            let head = String::from_utf8_lossy(&buf);
            let path = head.split_whitespace().nth(1).unwrap_or("");
            let (content_type, body, hold) = if path.ends_with("/logo.svg") {
                seen.store(true, Ordering::SeqCst);
                (
                    "image/svg+xml",
                    r#"<svg xmlns="http://www.w3.org/2000/svg" width="140" height="22"></svg>"#,
                    true,
                )
            } else if path.ends_with("/hold.js") {
                thread::sleep(Duration::from_millis(200));
                ("text/javascript", "/* hold */", false)
            } else if path.ends_with("/tail.js") {
                thread::sleep(Duration::from_millis(1500));
                ("text/javascript", "window.tailRan = true;", false)
            } else {
                (
                    "text/html",
                    r#"<!doctype html><img src="/logo.svg" alt="logo"><script src="/hold.js"></script><script>
var s = document.createElement('script');
s.src = '/tail.js';
document.head.appendChild(s);
</script>"#,
                    false,
                )
            };
            if hold {
                let started = std::time::Instant::now();
                while !release.load(Ordering::SeqCst) && started.elapsed() < Duration::from_secs(8)
                {
                    thread::sleep(Duration::from_millis(10));
                }
            }
            let bytes = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = sock.write_all(bytes.as_bytes());
        }

        let mut engine =
            super::super::Engine::new(super::super::EngineConfig::default()).expect("engine");
        let view = engine
            .create_headless_view(rustkit_viewhost::Bounds {
                x: 0,
                y: 0,
                width: 200,
                height: 80,
            })
            .expect("view");
        let url = url::Url::parse(&format!("http://127.0.0.1:{port}/")).unwrap();
        let yielded = drive(async { engine.load_url_yielding(view, url).await.expect("yield") });
        assert_eq!(
            yielded,
            super::super::LoadYield::Yielded,
            "the inserted script is still out, so the load returns at DOMContentLoaded"
        );
        assert!(
            seen.load(Ordering::SeqCst),
            "the image request was started before the load returned"
        );
        assert!(
            matches!(
                engine.views.get(&view).unwrap().pending_parser_images,
                Some(super::super::ParkedImages::Fetching(_))
            ),
            "the image fetch is still in flight when load_url_yielding returns"
        );
        release.store(true, Ordering::SeqCst);

        let started = std::time::Instant::now();
        loop {
            let done = engine
                .views
                .get(&view)
                .is_some_and(|state| state.load_tail.is_none())
                && !engine.views[&view].navigation.is_loading();
            if done {
                break;
            }
            assert!(
                started.elapsed() < Duration::from_secs(8),
                "the yielded load did not finish"
            );
            drive(async {
                engine.pump_live(view, 16).await;
            });
            thread::sleep(Duration::from_millis(20));
        }

        assert!(
            engine
                .svg_cache
                .keys()
                .any(|key| key.ends_with("/logo.svg")),
            "the image body arrived after the load returned: {:?}",
            engine.svg_cache.keys().collect::<Vec<_>>()
        );
        drive(async {
            tokio::time::timeout(
                Duration::from_secs(2),
                tokio::time::sleep(Duration::from_millis(20)),
            )
            .await
            .expect("the host runtime is still driving timers");
        });
        stop.store(true, Ordering::SeqCst);
    }
}
