//! JetStream 2.2's five WebAssembly tests, run in a bare `JsRuntime` the way
//! JetStream's own `WasmBenchmark` runs them (JetStreamDriver.js: its
//! `prerunCode`, `Module.wasmBinary`, then the test's script, then `doRun()` from a
//! promise job). A quick loop for WASM W1 phase 5, before the full in-engine
//! run through tools/stress/run.py.
//!
//! Ignored by default: it needs the pinned suite checkout (tools/stress/fetch.sh,
//! `$STRESS_SUITES`, default ~/zg-measure/stress-suites), which is never
//! committed to this repository.
//!
//! ```text
//! cargo test -p rustkit-js --release --test jetstream_wasm -- --ignored --nocapture
//! JETSTREAM_WASM_ONLY=richards cargo test ...        # one test
//! ```

use std::path::PathBuf;
use std::time::Instant;

use rustkit_js::JsRuntime;

const TESTS: [&str; 5] = ["HashSet", "tsf", "quicksort", "gcc-loops", "richards"];

/// JetStream 2.2's `WasmBenchmark.prerunCode`, with `top.currentResolve`
/// recording the result instead of resolving the driver's promise.
const PRERUN: &str = r#"
    var top = { currentResolve: function (r) { globalThis.__jsResult = r; } };
    var __jsError = null;
    let compileTime = null;
    let runTime = null;
    let globalObject = this;
    globalObject.benchmarkTime = Date.now.bind(Date);
    globalObject.reportCompileTime = (t) => {
        if (compileTime !== null) throw new Error("called report compile time twice");
        compileTime = t;
    };
    globalObject.reportRunTime = (t) => {
        if (runTime !== null) throw new Error("called report run time twice");
        runTime = t;
        top.currentResolve([compileTime, runTime]);
    };
    abort = quit = function () {};
    globalObject.print = globalObject.printErr = () => {};
    let Module = {
        preRun: [], postRun: [], print: function () {}, printErr: function () {},
        setStatus: null, totalDependencies: 0, monitorRunDependencies: null
    };
    globalObject.Module = Module;
"#;

fn suite_dir() -> PathBuf {
    std::env::var_os("STRESS_SUITES")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var("HOME").expect("HOME")).join("zg-measure/stress-suites"))
        .join("JetStream/wasm")
}

/// One test: `Ok((compile ms, run ms, wall ms))` as the test reports them, or why it failed.
fn run(name: &str) -> Result<(f64, f64, u128), String> {
    let dir = suite_dir();
    let wasm = std::fs::read(dir.join(format!("{name}.wasm"))).map_err(|e| format!("read wasm: {e}"))?;
    let script = std::fs::read_to_string(dir.join(format!("{name}.js"))).map_err(|e| format!("read js: {e}"))?;
    let mut rt = JsRuntime::new().map_err(|e| format!("runtime: {e:?}"))?;
    rt.enable_webassembly().map_err(|e| format!("enable: {e:?}"))?;
    // The engine's Annex B shims (String.prototype.substr, ...), which a page
    // gets from rustkit-bindings; emscripten's PATH helpers call substr.
    let legacy = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../rustkit-bindings/src/web_legacy.js"))
        .map_err(|e| format!("web_legacy.js: {e}"))?;
    rt.evaluate_script(&legacy).map_err(|e| format!("web_legacy: {e:?}"))?;
    rt.evaluate_script(PRERUN).map_err(|e| format!("prerun: {e:?}"))?;
    // Diagnosis: what JetStream's silenced abort/quit/print were told (kept
    // separately; the prerun itself is JetStream's, unchanged).
    if std::env::var_os("JETSTREAM_WASM_LOG").is_some() {
        rt.evaluate_script("globalThis.__jsLog = []; var __log = (k) => (...a) => { if (__jsLog.length < 40) __jsLog.push(k + ':' + a.join(' ')); }; \
            abort = quit = __log('abort'); globalObject.print = globalObject.printErr = __log('print'); Module.print = __log('Mprint'); Module.printErr = __log('Merr');")
            .map_err(|e| format!("log: {e:?}"))?;
    }
    let bytes: Vec<String> = wasm.iter().map(u8::to_string).collect();
    rt.evaluate_script(&format!("Module.wasmBinary = new Uint8Array([{}]).buffer;", bytes.join(",")))
        .map_err(|e| format!("binary: {e:?}"))?;
    let started = Instant::now();
    rt.evaluate_script(&script).map_err(|e| format!("script: {e:?}"))?;
    rt.evaluate_script("Promise.resolve(42).then(() => { try { doRun(); } catch (e) { __jsError = String(e) + ' | ' + String(e && e.stack); } });")
        .map_err(|e| format!("schedule: {e:?}"))?;
    rt.run_jobs().map_err(|e| format!("jobs: {e:?}"))?;
    let wall = started.elapsed().as_millis();
    let out = rt
        .evaluate_script("JSON.stringify([globalThis.__jsResult || null, __jsError, globalThis.__jsLog || []])")
        .map_err(|e| format!("result: {e:?}"))?;
    let out = format!("{out:?}");
    let parse = |s: &str| -> Option<(f64, f64)> {
        let i = s.find("[[")? + 2;
        let j = s[i..].find(']')? + i;
        let mut it = s[i..j].split(',').map(|v| v.trim().parse::<f64>().ok());
        Some((it.next()??, it.next()??))
    };
    match parse(&out) {
        Some((compile, run)) => Ok((compile, run, wall)),
        None => Err(format!("no result after {wall} ms: {out}")),
    }
}

#[test]
#[ignore = "needs the pinned JetStream checkout (tools/stress/fetch.sh)"]
fn jetstream_wasm_tests_run_to_their_result() {
    let only = std::env::var("JETSTREAM_WASM_ONLY").ok();
    let mut failed = Vec::new();
    for name in TESTS.iter().filter(|n| only.as_deref().is_none_or(|o| o == **n)) {
        // Each on its own big-stack thread, like the engine's script thread.
        let n = name.to_string();
        let result = std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(move || run(&n))
            .expect("thread")
            .join()
            .unwrap_or_else(|_| Err("panicked".into()));
        match result {
            Ok((compile, run, wall)) => println!("JETSTREAM-WASM {name}: ok compile {compile} ms, run {run} ms, wall {wall} ms"),
            Err(why) => {
                println!("JETSTREAM-WASM {name}: FAIL {why}");
                failed.push(*name);
            }
        }
    }
    assert!(failed.is_empty(), "failed: {failed:?}");
}
