//! `HIWAVE_CONSOLE_LOG=<path>`: the page's console calls, uncaught
//! exceptions and unhandled rejections as JSON lines in a file of their
//! own. Unset: no file and nothing new in the tracing log. Set: the page's
//! text is in the file only, never in the tracing log or the Inspector.

use super::inspector_engine_tests::{inspected_engine, runtime, serve, Route};
use super::*;
use crate::console_file::{Forced, MAX_FILE_BYTES, MAX_MESSAGE_BYTES};
use std::path::PathBuf;

/// Puts this thread's switches back when a test ends, and removes its file.
struct Reset(PathBuf);

impl Drop for Reset {
    fn drop(&mut self) {
        crate::console_file::force_for_test(None);
        crate::console_log::force_for_test(None);
        crate::console_log::install_sink(None);
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Removes one more file when a test ends.
struct Remove(PathBuf);

impl Drop for Remove {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// The tracing log's warnings about the console file.
fn console_file_warnings(log: &str) -> Vec<&str> {
    log.lines().filter(|line| line.contains(" WARN ") && line.contains("console log")).collect()
}

/// A file name no other test or run uses; the file does not exist.
fn scratch(tag: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("hiwave-console-file-{tag}-{}.jsonl", std::process::id()));
    let _ = std::fs::remove_file(&path);
    path
}

#[derive(Clone, Default)]
struct Captured(Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Everything this thread wrote to the tracing log, at every level, while
/// `run` ran.
fn tracing_log(run: impl FnOnce()) -> String {
    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_max_level(tracing::Level::TRACE)
        .with_writer(move || writer.clone())
        .finish();
    tracing::subscriber::with_default(subscriber, run);
    let bytes = captured.0.lock().unwrap().clone();
    String::from_utf8(bytes).unwrap()
}

/// Logs, rejects and throws, each with text only the page knows.
const PAGE: &str = "<html><body><script>
console.error('secret-console-text', 12);
Promise.reject(new RangeError('secret-rejected-text'));
throw new TypeError('secret-thrown-text');
</script></body></html>";

/// Load `path` of a server with `routes` in a view nobody inspects.
fn load(routes: Vec<Route>, path: &str) -> u16 {
    let server = serve(routes);
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine.create_headless_view(Bounds { x: 0, y: 0, width: 300, height: 200 }).expect("view");
    runtime().block_on(engine.load_url(view, server.url(path))).expect("load_url");
    server.port
}

/// The file's lines, each one JSON object.
fn lines(path: &PathBuf) -> Vec<serde_json::Value> {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("no console file at {}: {e}", path.display()));
    assert!(text.is_empty() || text.ends_with('\n'), "the last line is not ended: {text:?}");
    text.lines().map(|line| serde_json::from_str(line).unwrap_or_else(|e| panic!("not a JSON line ({e}): {line}"))).collect()
}

fn of_kind<'a>(lines: &'a [serde_json::Value], kind: &str) -> &'a serde_json::Value {
    let found: Vec<_> = lines.iter().filter(|line| line["kind"] == kind).collect();
    assert_eq!(found.len(), 1, "one {kind} line expected in {lines:#?}");
    found[0]
}

#[test]
fn only_a_value_with_a_slash_names_a_file() {
    use crate::console_file::{setting, Setting};
    use std::ffi::OsStr;
    // `1` is the tracing mirror's switch (console_log); it and every other
    // bare word stay what they were before this file existed: not a path.
    for off in ["", "0", "1", "true", "on"] {
        assert_eq!(setting(OsStr::new(off)), Setting::Off, "{off:?}");
    }
    assert_eq!(setting(OsStr::new("/tmp/c.jsonl")), Setting::File(PathBuf::from("/tmp/c.jsonl")));
    assert_eq!(setting(OsStr::new("./c.jsonl")), Setting::File(PathBuf::from("./c.jsonl")));
}

// ---- The process's own sink: `set_path`, the variable, a file that
// cannot be opened ----

fn configured(set_path: Option<&PathBuf>, env: Option<&str>) -> Forced {
    Forced::Configured { set_path: set_path.cloned(), env: env.map(Into::into) }
}

#[test]
fn the_variable_alone_names_the_file() {
    let path = scratch("env");
    let _reset = Reset(path.clone());
    crate::console_file::force_for_test(Some(configured(None, path.to_str())));
    load(vec![Route::html("/", PAGE)], "/");
    assert_eq!(lines(&path).len(), 3);
}

#[test]
fn set_path_wins_over_the_variable() {
    let (chosen, env) = (scratch("chosen"), scratch("not-chosen"));
    let (_reset, _other) = (Reset(chosen.clone()), Remove(env.clone()));
    crate::console_file::force_for_test(Some(configured(Some(&chosen), env.to_str())));
    load(vec![Route::html("/", PAGE)], "/");
    assert_eq!(lines(&chosen).len(), 3);
    assert!(!env.exists(), "the variable's file was made although set_path chose another");
}

#[test]
fn the_tracing_mirrors_values_and_no_value_make_no_file() {
    let path = scratch("legacy");
    let _reset = Reset(path.clone());
    for env in [None, Some("1"), Some("0"), Some("")] {
        let log = tracing_log(|| {
            crate::console_file::force_for_test(Some(configured(None, env)));
            assert!(!crate::console_file::enabled(), "{env:?} turned the file on");
            load(vec![Route::html("/", PAGE)], "/");
        });
        assert!(console_file_warnings(&log).is_empty(), "{env:?} was warned about:\n{log}");
    }
}

/// A file that cannot be opened: the page loads, nothing is written, and
/// the log says so once, as a shape (no path, no page text).
#[test]
fn a_file_that_cannot_be_opened_is_said_once_and_the_page_still_loads() {
    let missing = std::env::temp_dir().join(format!("hiwave-console-file-no-such-dir-{}", std::process::id()));
    let path = missing.join("console.jsonl");
    let _reset = Reset(path.clone());
    let log = tracing_log(|| {
        crate::console_file::force_for_test(Some(configured(None, path.to_str())));
        assert!(!crate::console_file::enabled());
        load(vec![Route::html("/", PAGE)], "/");
    });
    assert!(!path.exists() && !missing.exists());
    assert_eq!(console_file_warnings(&log).len(), 1, "{log}");
    assert!(!log.contains("no-such-dir") && !log.contains("secret"), "{log}");
}

/// The real thing, in a process of its own: the variable is read from the
/// environment and `set_path` is the process's. Each case runs this test
/// binary again with only `process_sink_child` in it.
#[test]
fn the_process_reads_the_variable_and_set_path_wins() {
    let (from_env, chosen) = (scratch("process-env"), scratch("process-chosen"));
    let (_a, _b) = (Remove(from_env.clone()), Remove(chosen.clone()));
    let child = |set_path: Option<&PathBuf>| {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "console_file_tests::process_sink_child", "--ignored", "--test-threads=1"])
            .env("HIWAVE_CONSOLE_LOG", &from_env)
            .env_remove("HIWAVE_CONSOLE_FILE_TEST_SET_PATH");
        if let Some(path) = set_path {
            command.env("HIWAVE_CONSOLE_FILE_TEST_SET_PATH", path);
        }
        let out = command.output().expect("the test binary runs again");
        assert!(out.status.success(), "{}\n{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    };
    child(None);
    assert_eq!(lines(&from_env).len(), 3, "the variable's file");
    std::fs::remove_file(&from_env).unwrap();
    child(Some(&chosen));
    assert_eq!(lines(&chosen).len(), 3, "set_path's file");
    assert!(!from_env.exists(), "the variable's file was made although set_path chose another");
}

/// Run only by `the_process_reads_the_variable_and_set_path_wins`.
#[test]
#[ignore = "a child process of the_process_reads_the_variable_and_set_path_wins"]
fn process_sink_child() {
    if let Some(path) = std::env::var_os("HIWAVE_CONSOLE_FILE_TEST_SET_PATH") {
        crate::console_file::set_path(path);
    }
    crate::console_file::force_for_test(Some(Forced::Process));
    assert!(crate::console_file::enabled());
    load(vec![Route::html("/", PAGE)], "/");
}

#[test]
fn with_the_switch_off_there_is_no_file_and_no_page_text_in_the_log() {
    let path = scratch("off");
    let _reset = Reset(path.clone());
    crate::console_file::force_for_test(Some(Forced::Off));
    crate::console_log::force_for_test(Some(false));
    let mirrored = Arc::new(std::sync::Mutex::new(Vec::new()));
    crate::console_log::install_sink(Some(Arc::clone(&mirrored)));
    let log = tracing_log(|| {
        load(vec![Route::html("/", PAGE)], "/");
    });
    assert!(!path.exists(), "a console file was made with the switch off");
    assert!(!log.contains("secret"), "page text in the tracing log:\n{log}");
    assert!(!log.contains("hiwave::console"), "a console line in the tracing log:\n{log}");
    assert!(mirrored.lock().unwrap().is_empty(), "the mirror wrote with both switches off");
}

#[test]
fn with_the_switch_on_the_file_has_the_console_error_the_throw_and_the_rejection() {
    let path = scratch("on");
    let _reset = Reset(path.clone());
    crate::console_file::force_for_test(Some(Forced::On(path.clone(), MAX_FILE_BYTES)));
    crate::console_log::force_for_test(Some(false));
    let mirrored = Arc::new(std::sync::Mutex::new(Vec::new()));
    crate::console_log::install_sink(Some(Arc::clone(&mirrored)));
    let mut port = 0;
    let log = tracing_log(|| port = load(vec![Route::html("/", PAGE)], "/"));
    let lines = lines(&path);
    assert_eq!(lines.len(), 3, "{lines:#?}");
    let page = format!("http://127.0.0.1:{port}/");

    let console = of_kind(&lines, "console");
    assert_eq!(console["level"], "error");
    assert_eq!(console["message"], "secret-console-text 12");
    assert_eq!(console["page"], page.as_str());
    assert_eq!(console["source"], "inline#1");
    // Where a browser puts it: the method's name, not its `(`.
    assert_eq!((console["line"].as_u64(), console["column"].as_u64()), (Some(2), Some(9)));

    let thrown = of_kind(&lines, "uncaught");
    assert_eq!(thrown["level"], "error");
    assert_eq!(thrown["message"], "Uncaught TypeError: secret-thrown-text");
    assert_eq!(thrown["source"], "inline#1");
    assert_eq!(thrown["line"].as_u64(), Some(4));
    assert!(thrown["column"].as_u64().is_some_and(|column| column >= 1), "{thrown}");

    let rejected = of_kind(&lines, "unhandled_rejection");
    assert_eq!(rejected["level"], "error");
    assert_eq!(rejected["message"], "Uncaught (in promise) RangeError: secret-rejected-text");
    assert_eq!(rejected["line"].as_u64(), Some(3));

    for line in &lines {
        assert!(line["view"].as_u64().is_some_and(|view| view >= 1), "no view id: {line}");
        assert_eq!(line["page"], page.as_str());
        // Wall time, UTC: 2026-11-13T21:40:01.123Z.
        let t = line["t"].as_str().unwrap_or_else(|| panic!("no wall time: {line}"));
        assert!(t.len() == 24 && t.ends_with('Z') && &t[10..11] == "T" && t.starts_with("20"), "{t}");
    }

    // The file is the only place the text went.
    assert!(!log.contains("secret"), "page text in the tracing log:\n{log}");
    assert!(!log.contains("hiwave::console"), "a console line in the tracing log:\n{log}");
    assert!(mirrored.lock().unwrap().is_empty(), "the tracing mirror wrote with only the file on");
}

#[cfg(unix)]
#[test]
fn the_file_is_readable_by_its_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let path = scratch("mode");
    let _reset = Reset(path.clone());
    // A file somebody left group- and world-readable is narrowed too.
    std::fs::write(&path, "").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    crate::console_file::force_for_test(Some(Forced::On(path.clone(), MAX_FILE_BYTES)));
    load(vec![Route::html("/", PAGE)], "/");
    assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    let _ = std::fs::remove_file(&path);
    crate::console_file::force_for_test(Some(Forced::On(path.clone(), MAX_FILE_BYTES)));
    load(vec![Route::html("/", PAGE)], "/");
    assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
}

#[test]
fn the_page_and_script_urls_lose_their_query_fragment_and_credentials() {
    let path = scratch("query");
    let _reset = Reset(path.clone());
    crate::console_file::force_for_test(Some(Forced::On(path.clone(), MAX_FILE_BYTES)));
    let page = "<html><body><script src=\"/app.js?token=secret-script-query#secret-script-frag\"></script></body></html>";
    let script = "console.warn('from the script');\nthrow new Error('thrown by the script');";
    let port = load(
        vec![Route::html("/doc", page), Route::ok("/app.js", "text/javascript", script)],
        "/doc?session=secret-page-query#secret-page-frag",
    );
    let text = std::fs::read_to_string(&path).expect("console file");
    assert!(!text.contains("secret"), "a query or fragment reached the file:\n{text}");
    let lines = lines(&path);
    assert_eq!(lines.len(), 2, "{lines:#?}");
    for line in &lines {
        assert_eq!(line["page"], format!("http://127.0.0.1:{port}/doc").as_str());
        assert_eq!(line["source"], format!("http://127.0.0.1:{port}/app.js").as_str());
    }
    assert_eq!(of_kind(&lines, "console")["level"], "warn");
    assert_eq!(of_kind(&lines, "uncaught")["line"].as_u64(), Some(2));
}

#[test]
fn every_console_level_is_written_under_its_own_name() {
    let path = scratch("levels");
    let _reset = Reset(path.clone());
    crate::console_file::force_for_test(Some(Forced::On(path.clone(), MAX_FILE_BYTES)));
    let page = "<html><body><script>
console.log('a'); console.info('b'); console.warn('c'); console.error('d'); console.debug('e');
</script></body></html>";
    load(vec![Route::html("/", page)], "/");
    let levels: Vec<String> =
        lines(&path).iter().map(|line| format!("{} {}", line["level"].as_str().unwrap(), line["message"].as_str().unwrap())).collect();
    assert_eq!(levels, ["log a", "info b", "warn c", "error d", "debug e"]);
}

#[test]
fn a_long_message_is_cut_at_the_cap_on_a_character() {
    let path = scratch("long");
    let _reset = Reset(path.clone());
    crate::console_file::force_for_test(Some(Forced::On(path.clone(), MAX_FILE_BYTES)));
    // 20,000 one-byte characters, then 10,000 two-byte ones (the cap is an
    // even number of bytes, so the third line starts one byte off).
    let page = "<html><body><script>
console.log('x'.repeat(20000));
console.log('\\u00e9'.repeat(10000));
console.log('y' + '\\u00e9'.repeat(10000));
console.log('short');
</script></body></html>";
    load(vec![Route::html("/", page)], "/");
    let lines = lines(&path);
    assert_eq!(lines.len(), 4, "{} lines", lines.len());
    let message = |i: usize| lines[i]["message"].as_str().unwrap();
    assert_eq!(message(0), "x".repeat(MAX_MESSAGE_BYTES));
    assert_eq!(message(1), "\u{e9}".repeat(MAX_MESSAGE_BYTES / 2));
    assert_eq!(message(2), format!("y{}", "\u{e9}".repeat(MAX_MESSAGE_BYTES / 2 - 1)));
    for cut in &lines[..3] {
        assert_eq!(cut["cut"], true, "a cut message says so");
    }
    assert_eq!(message(3), "short");
    assert!(lines[3].get("cut").is_none(), "a whole message has no `cut`: {}", lines[3]);
}

#[test]
fn the_file_stops_at_its_size_cap_with_one_truncated_line() {
    let path = scratch("full");
    let _reset = Reset(path.clone());
    let cap = 4000;
    crate::console_file::force_for_test(Some(Forced::On(path.clone(), cap)));
    let page = "<html><body><script>
for (var i = 0; i < 500; i++) console.log('line ' + i + ' ' + 'z'.repeat(40));
</script></body></html>";
    load(vec![Route::html("/", page)], "/");
    let size = std::fs::metadata(&path).unwrap().len();
    assert!(size <= cap, "{size} bytes in a file capped at {cap}");
    let first = lines(&path);
    assert!(first.len() > 5 && first.len() < 500, "{} lines", first.len());
    let (last, written) = first.split_last().unwrap();
    assert_eq!(last["truncated"], true, "{last}");
    assert_eq!(last["limit_bytes"].as_u64(), Some(cap));
    assert!(last["t"].is_string(), "{last}");
    assert!(written.iter().all(|line| line.get("truncated").is_none() && line["kind"] == "console"));
    assert_eq!(written[0]["message"], format!("line 0 {}", "z".repeat(40)).as_str());

    // Full stays full: another page, and another process's open of the
    // same file, add nothing.
    load(vec![Route::html("/", page)], "/");
    assert_eq!(std::fs::metadata(&path).unwrap().len(), size);
    crate::console_file::force_for_test(Some(Forced::On(path.clone(), cap)));
    load(vec![Route::html("/", page)], "/");
    assert_eq!(std::fs::metadata(&path).unwrap().len(), size);
}

/// The file takes the page's text without changing what the Inspector is
/// handed: shapes only unless the Inspector's own settings keep text.
#[test]
fn with_the_file_on_the_inspector_still_gets_shapes_only() {
    let path = scratch("inspector");
    let _reset = Reset(path.clone());
    crate::console_file::force_for_test(Some(Forced::On(path.clone(), MAX_FILE_BYTES)));
    let server = serve(vec![Route::html("/", PAGE)]);
    let (mut engine, view) = inspected_engine(EngineConfig::default());
    runtime().block_on(engine.load_url(view, server.url("/"))).unwrap();
    let records = engine.inspector(view).expect("inspector").snapshot_since(0).records;
    let shown: Vec<String> = records
        .iter()
        .filter(|r| matches!(r.event.kind(), "console" | "script_error"))
        .map(|r| serde_json::to_string(r).unwrap())
        .collect();
    assert_eq!(shown.len(), 3, "{shown:#?}");
    for record in &shown {
        assert!(!record.contains("secret"), "page text reached the Inspector with the file on: {record}");
    }
    assert_eq!(lines(&path).len(), 3);
}
