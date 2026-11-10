//! H19c, second half: the error records for timers and lifecycle events hard-coded
//! `elapsed_ms: 0`, so an Inspector line for a slow callback that then threw said it took no time.
//! The record must carry the callback step's measured wall time, as a script's own record does.

use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;

fn load(script: &str) -> (Engine, EngineViewId) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let body = format!("<p>x</p><script>{script}</script>");
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let body = body.clone();
            std::thread::spawn(move || {
                let mut buf = [0u8; 2048];
                let mut request = Vec::new();
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    match stream.read(&mut buf) {
                        Ok(0) | Err(_) => return,
                        Ok(n) => request.extend_from_slice(&buf[..n]),
                    }
                }
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            });
        }
    });
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 300, height: 200 })
        .expect("view");
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(engine.load_url(view, Url::parse(&format!("http://127.0.0.1:{port}/")).unwrap()))
        .expect("load");
    (engine, view)
}

const SPIN_THEN_THROW: &str = "var t = Date.now(); while (Date.now() - t < 80) {} throw new Error('after the spin');";

#[test]
fn a_timer_that_spins_then_throws_records_its_time() {
    let (engine, view) = load(&format!("setTimeout(function () {{ {SPIN_THEN_THROW} }}, 0);"));
    let log = engine.script_log(view).unwrap();
    let record = log
        .iter()
        .find(|r| r.source == "timers" && matches!(r.outcome, ScriptOutcome::Threw(_)))
        .unwrap_or_else(|| panic!("no timers Threw record: {log:#?}"));
    assert!(record.elapsed_ms >= 60, "the 80 ms callback is recorded as {} ms: {log:#?}", record.elapsed_ms);
}

#[test]
fn a_load_listener_that_spins_then_throws_records_its_time() {
    let (engine, view) = load(&format!("window.addEventListener('load', function () {{ {SPIN_THEN_THROW} }});"));
    let log = engine.script_log(view).unwrap();
    let record = log
        .iter()
        .find(|r| r.source == "event:load" && matches!(r.outcome, ScriptOutcome::Threw(_)))
        .unwrap_or_else(|| panic!("no event:load Threw record: {log:#?}"));
    assert!(record.elapsed_ms >= 60, "the 80 ms listener is recorded as {} ms: {log:#?}", record.elapsed_ms);
}
