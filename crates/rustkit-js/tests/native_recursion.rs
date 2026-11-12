//! A page must never be able to kill the browser: deep structures handed to native built-ins
//! recursed in Rust until the real stack overflowed (`var a=[]; for(...) a=[a]; JSON.stringify(a)`).
//! Chrome throws a catchable RangeError (or, where V8 is iterative, just answers).
//!
//! Each case runs in a child process (a stack overflow aborts the process, so it cannot be caught
//! in the test itself). Expected values: Chrome for Testing 148.0.7778.216, from
//! `docs/diagnostics/2026-11-12/native_recursion_cases.mjs`.

use rustkit_js::{JsRuntime, JsValue};
use std::collections::BTreeMap;
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const CASES: &str = include_str!("../../../docs/diagnostics/2026-11-12/native_recursion_cases.json");
const CHROME: &str = include_str!("../../../docs/diagnostics/2026-11-12/native_recursion_cft148.json");

/// Cases where Chrome answers and this engine throws a RangeError instead: safe (no abort), and
/// a divergence only in how deep a structure may be.
const SAFE_DIVERGENCES: &[&str] = &[];

fn child_run(body: &str) -> String {
    let mut rt = JsRuntime::new().unwrap();
    let src = format!(
        "(function () {{ try {{ return 'ok:' + String((function () {{ {body} }})()); }} catch (e) {{ return 'ERR:' + e.name; }} }})()"
    );
    match rt.evaluate_script(&src) {
        Ok(JsValue::String(s)) => s,
        Ok(other) => format!("?{other:?}"),
        Err(e) => format!("UNCAUGHT:{e}"),
    }
}

/// The child half: runs the case named by NREC_CASE and prints its result.
#[test]
fn child() {
    let Ok(name) = std::env::var("NREC_CASE") else { return };
    let cases: BTreeMap<String, String> = serde_json::from_str(CASES).unwrap();
    // A big-enough thread, as the page's own script thread is not the test harness's 2 MiB one.
    let body = cases[&name].clone();
    let out = child_run(&body);
    println!("NREC_RESULT:{out}");
}

fn isolated(name: &str) -> String {
    let exe = std::env::current_exe().unwrap();
    let mut child = Command::new(exe)
        .args(["child", "--exact", "--nocapture", "--test-threads=1"])
        .env("NREC_CASE", name)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let start = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait().unwrap() {
            break Some(s);
        }
        if start.elapsed() > Duration::from_secs(60) {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let mut out = String::new();
    child.stdout.take().unwrap().read_to_string(&mut out).unwrap();
    match status {
        None => "TIMEOUT".to_string(),
        Some(s) => match out.lines().find_map(|l| l.split_once("NREC_RESULT:").map(|(_, r)| r)) {
            Some(r) => r.to_string(),
            None => format!("CRASH(status {s})"),
        },
    }
}

#[test]
fn no_deep_structure_kills_the_process_and_the_answers_are_chromes() {
    let cases: BTreeMap<String, String> = serde_json::from_str(CASES).unwrap();
    let mut want: BTreeMap<String, String> = serde_json::from_str(CHROME).unwrap();
    assert_eq!(cases.len(), want.len());
    // structuredClone is the bindings' (web_blob.js), pinned in rustkit-bindings.
    want.retain(|k, _| !k.starts_with("structured_clone"));
    let mut crashed = Vec::new();
    let mut differ = Vec::new();
    for (name, expected) in &want {
        let got = isolated(name);
        if got.starts_with("CRASH") || got == "TIMEOUT" {
            crashed.push(format!("{name}: {got} (chrome {expected})"));
        } else if &got != expected && !(SAFE_DIVERGENCES.contains(&name.as_str()) && got == "ERR:RangeError") {
            differ.push(format!("{name}: chrome {expected:?}, here {got:?}"));
        }
    }
    assert!(
        crashed.is_empty() && differ.is_empty(),
        "{} aborted or hung, {} differ:\n{}\n{}",
        crashed.len(),
        differ.len(),
        crashed.join("\n"),
        differ.join("\n")
    );
}
