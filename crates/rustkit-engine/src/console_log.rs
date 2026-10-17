//! Optional mirror of page console output and script errors into the tracing
//! log. Off unless `HIWAVE_CONSOLE_LOG=1`.
//!
//! When it is off, every function here returns before taking a lock, reading
//! an error report, or writing a line. The Inspector is unchanged: it stays
//! off unless something else turns it on, and this mirror does not use its
//! URL scrubbing. Script names such as `inserted#N` are recorded here, only
//! while the switch is on, so a line can show the script's URL.

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};

use rustkit_js::{ErrorReport, LogLevel, ScriptPosition};

use crate::inspector::{cap, MAX_MESSAGE_CHARS, MAX_STACK_CHARS};

const ALIAS_CAP: usize = 1024;

struct AliasMap {
    entries: VecDeque<(String, String)>,
}

fn aliases() -> &'static Mutex<AliasMap> {
    static ALIASES: OnceLock<Mutex<AliasMap>> = OnceLock::new();
    ALIASES.get_or_init(|| Mutex::new(AliasMap { entries: VecDeque::new() }))
}

#[cfg(test)]
thread_local! {
    static FORCE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
    static SINK: std::cell::RefCell<Option<std::sync::Arc<Mutex<Vec<String>>>>> =
        const { std::cell::RefCell::new(None) };
}

/// `HIWAVE_CONSOLE_LOG=1`. Empty, `0`, and anything else are off.
pub(crate) fn enabled() -> bool {
    #[cfg(test)]
    if let Some(forced) = FORCE.with(std::cell::Cell::get) {
        return forced;
    }
    std::env::var("HIWAVE_CONSOLE_LOG").ok().as_deref() == Some("1")
}

#[cfg(test)]
pub(crate) fn force_for_test(on: Option<bool>) {
    FORCE.with(|cell| cell.set(on));
}

#[cfg(test)]
pub(crate) fn install_sink(sink: Option<std::sync::Arc<Mutex<Vec<String>>>>) {
    SINK.with(|slot| *slot.borrow_mut() = sink);
}

/// `name` (an engine script label) was loaded from `url`. Ignored when the
/// switch is off.
pub(crate) fn alias(name: &str, url: &str) {
    if !enabled() {
        return;
    }
    let Ok(mut map) = aliases().lock() else {
        return;
    };
    if map.entries.len() >= ALIAS_CAP {
        map.entries.pop_front();
    }
    map.entries.push_back((name.to_string(), url.to_string()));
}

fn resolve(path: &str) -> String {
    let Ok(map) = aliases().lock() else {
        return path.to_string();
    };
    map.entries
        .iter()
        .rev()
        .find(|(name, _)| name == path)
        .map(|(_, url)| url.clone())
        .unwrap_or_else(|| path.to_string())
}

fn emit(line: String) {
    #[cfg(test)]
    SINK.with(|slot| {
        if let Some(sink) = slot.borrow().clone() {
            if let Ok(mut lines) = sink.lock() {
                lines.push(line.clone());
            }
        }
    });
    tracing::info!(target: "hiwave::console", "{line}");
}

pub(crate) fn console_message(level: LogLevel, message: &str, at: Option<&ScriptPosition>) {
    if !enabled() {
        return;
    }
    let level = match level {
        LogLevel::Log => "log",
        LogLevel::Info => "info",
        LogLevel::Warn => "warn",
        LogLevel::Error => "error",
        LogLevel::Debug => "debug",
    };
    let (message, _) = cap(message, MAX_MESSAGE_CHARS);
    let where_at = match at {
        Some(at) => format!("{}:{}:{}", resolve(&at.path), at.line, at.column),
        None => "-".to_string(),
    };
    emit(format!("console {level} {where_at} {message}"));
}

pub(crate) fn page_error(report: &ErrorReport, script: &str) {
    if !enabled() {
        return;
    }
    let kind = if report.rejection { "unhandled rejection" } else { "uncaught" };
    let name = report.name.unwrap_or("Error");
    let (message, _) = cap(report.message.as_deref().unwrap_or(""), MAX_MESSAGE_CHARS);
    let (url, line, column) = report.frames.first().map(|frame| (resolve(&frame.path), frame.line, frame.column)).unwrap_or_else(
        || (resolve(report.module.as_deref().unwrap_or(script)), 0, 0),
    );
    let stack = if report.frames.is_empty() {
        String::new()
    } else {
        let lines: Vec<String> = report
            .frames
            .iter()
            .map(|frame| format!("    at {}:{}:{}", resolve(&frame.path), frame.line, frame.column))
            .collect();
        let (stack, _) = cap(&lines.join("\n"), MAX_STACK_CHARS);
        format!("\n{stack}")
    };
    emit(format!("{kind} {name}: {message} at {url}:{line}:{column}{stack}"));
}

pub(crate) fn inserted_script(url: &str, event: &str) {
    if !enabled() {
        return;
    }
    emit(format!("inserted script {event} {url}"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            force_for_test(None);
            install_sink(None);
        }
    }

    #[test]
    fn a_line_is_written_only_while_the_switch_is_forced_on() {
        let _reset = Reset;
        force_for_test(Some(true));
        let sink = Arc::new(Mutex::new(Vec::new()));
        install_sink(Some(Arc::clone(&sink)));
        alias("chunk#console-log-unit", "https://cdn.example/ok.js");
        alias("inline#console-log-unit", "https://page.example/");
        console_message(
            LogLevel::Warn,
            "hello-console",
            Some(&ScriptPosition { path: "inline#console-log-unit".into(), line: 2, column: 5 }),
        );
        console_message(
            LogLevel::Log,
            "from-chunk",
            Some(&ScriptPosition { path: "chunk#console-log-unit".into(), line: 1, column: 1 }),
        );
        page_error(
            &ErrorReport {
                value: "error",
                name: Some("Error"),
                message: Some("boom-inline".into()),
                message_len: Some(11),
                frames: vec![ScriptPosition { path: "inline#console-log-unit".into(), line: 4, column: 1 }],
                located: true,
                rejection: false,
                module: None,
            },
            "inline#console-log-unit",
        );
        page_error(
            &ErrorReport {
                value: "error",
                name: Some("Error"),
                message: Some("boom-reject".into()),
                message_len: Some(11),
                frames: vec![ScriptPosition { path: "inline#console-log-unit".into(), line: 3, column: 1 }],
                located: true,
                rejection: true,
                module: None,
            },
            "inline#console-log-unit",
        );
        inserted_script("https://cdn.example/ok.js", "load");
        inserted_script("https://cdn.example/missing.js", "error");
        let lines = sink.lock().unwrap().clone();
        let text = lines.join("\n");
        assert!(text.contains("console warn https://page.example/:2:5 hello-console"), "{text}");
        assert!(text.contains("console log https://cdn.example/ok.js:1:1 from-chunk"), "{text}");
        assert!(text.contains("uncaught Error: boom-inline at https://page.example/:4:1"), "{text}");
        assert!(text.contains("    at https://page.example/:4:1"), "{text}");
        assert!(text.contains("unhandled rejection Error: boom-reject at https://page.example/:3:1"), "{text}");
        assert!(text.contains("inserted script load https://cdn.example/ok.js"), "{text}");
        assert!(text.contains("inserted script error https://cdn.example/missing.js"), "{text}");
        assert!(!text.contains("chunk#console-log-unit"), "{text}");

        force_for_test(Some(false));
        let before = sink.lock().unwrap().len();
        console_message(LogLevel::Log, "quiet", None);
        page_error(
            &ErrorReport {
                value: "error",
                name: Some("Error"),
                message: Some("nope".into()),
                message_len: Some(4),
                frames: Vec::new(),
                located: false,
                rejection: false,
                module: None,
            },
            "inline#1",
        );
        inserted_script("https://cdn.example/ok.js", "load");
        alias("chunk#console-log-unit", "https://cdn.example/other.js");
        assert_eq!(sink.lock().unwrap().len(), before);
    }
}

#[cfg(all(test, feature = "headless"))]
mod engine_tests {
    use super::*;
    use crate::error_page_script_tests::serve;
    use crate::{Bounds, Engine, EngineConfig};
    use std::sync::{Arc, Mutex};

    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            force_for_test(None);
            install_sink(None);
        }
    }

    const PAGE: &str = r#"<!doctype html><html><head></head><body>
<script>
console.warn('hello-console');
var ok = document.createElement('script');
ok.src = '/ok.js';
document.head.appendChild(ok);
var missing = document.createElement('script');
missing.src = '/missing.js';
document.head.appendChild(missing);
Promise.reject(new Error('boom-reject'));
setTimeout(function () { throw new Error('boom-timer'); }, 10);
</script>
<script>
throw new Error('boom-inline');
</script>
</body></html>"#;

    fn site(path: &str, _: &str) -> (u16, &'static str, Vec<(&'static str, &'static str)>, String) {
        match path {
            "/ok.js" => (200, "text/javascript", vec![], "console.log('from-chunk');".into()),
            "/missing.js" => (404, "text/plain", vec![], "missing".into()),
            _ => (200, "text/html", vec![], PAGE.into()),
        }
    }

    #[test]
    fn mirrors_console_exceptions_rejections_and_inserted_scripts() {
        let _reset = Reset;
        force_for_test(Some(true));
        let sink = Arc::new(Mutex::new(Vec::new()));
        install_sink(Some(Arc::clone(&sink)));
        let server = serve(site);
        let page = server.url("/").to_string();
        let mut engine = Engine::new(EngineConfig { timer_horizon_ms: 0, ..EngineConfig::default() }).expect("engine");
        let view = engine
            .create_headless_view(Bounds { x: 0, y: 0, width: 400, height: 800 })
            .expect("view");
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(engine.load_url(view, server.url("/"))).expect("load");
        let started = std::time::Instant::now();
        let text = loop {
            rt.block_on(engine.pump_live(view, 50));
            let text = sink.lock().unwrap().join("\n");
            if text.contains("boom-timer") && text.contains("from-chunk") && text.contains("inserted script error") {
                break text;
            }
            assert!(
                started.elapsed() < std::time::Duration::from_secs(5),
                "console log missing events:\n{text}"
            );
        };
        let warn = text.lines().find(|line| line.contains("hello-console")).unwrap_or("");
        assert!(warn.contains(&page), "console warn should name the page\n{warn}\n{text}");
        assert!(text.contains("uncaught") && text.contains("boom-inline"), "{text}");
        assert!(text.contains("unhandled rejection") && text.contains("boom-reject"), "{text}");
        let inline = text.lines().find(|line| line.contains("boom-inline")).unwrap_or("");
        assert!(inline.contains(&page), "uncaught stack should name the page\n{inline}");
        assert!(inline.contains("at "), "{inline}");
        assert!(text.contains("inserted script load") && text.contains("/ok.js"), "{text}");
        assert!(text.contains("inserted script error") && text.contains("/missing.js"), "{text}");
        let chunk = text.lines().find(|line| line.contains("from-chunk")).unwrap_or("");
        assert!(chunk.contains("/ok.js"), "{chunk}");
        assert!(!chunk.contains("inserted#"), "{chunk}");
        assert!(text.contains("boom-timer"), "{text}");
    }

    #[test]
    fn stays_quiet_when_the_switch_is_unset() {
        if std::env::var("HIWAVE_CONSOLE_LOG").ok().as_deref() == Some("1") {
            return;
        }
        let _reset = Reset;
        force_for_test(None);
        let sink = Arc::new(Mutex::new(Vec::new()));
        install_sink(Some(Arc::clone(&sink)));
        let mut engine = Engine::new(EngineConfig::default()).expect("engine");
        let view = engine
            .create_headless_view(Bounds { x: 0, y: 0, width: 300, height: 200 })
            .expect("view");
        engine
            .load_html(
                view,
                "<script>console.log('should-stay-quiet'); throw new Error('nope');</script>",
            )
            .expect("load");
        let lines = sink.lock().unwrap().clone();
        assert!(lines.is_empty(), "{lines:?}");
    }
}
