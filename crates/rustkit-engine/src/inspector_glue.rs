//! Where the page's script reaches the Inspector: the console handler, the
//! page errors the JS runtime describes (exceptions, unhandled rejections,
//! modules that threw after a top-level `await`) and the unimplemented
//! members the bindings saw. Every function here returns at once when the
//! view's Inspector has nobody listening.
//!
//! An error record is built from the runtime's report only, never from the
//! script log's text: the report holds no text the page chose unless error
//! text is asked for (see `rustkit_js::ErrorReport`).

use std::sync::Arc;

use rustkit_bindings::DomBindings;

use crate::inspector::{cap, ConsoleLevel, Inspector, InspectorEvent, SourceLocation, MAX_MESSAGE_CHARS, MAX_STACK_CHARS};
use crate::{EngineFailure, ScriptOutcome, ScriptRecord};

/// Set up a document's new bindings for a listening Inspector: console
/// calls with their positions, error reports, unsupported members' callers.
/// Nothing is set up (and nothing costs anything) when nobody listens.
pub(crate) fn attach(bindings: &DomBindings, inspector: &Arc<Inspector>) {
    if !inspector.is_active() {
        return;
    }
    let sink = inspector.clone();
    bindings.set_console_redaction(!inspector.keeps_console_text());
    bindings.set_console_handler(Box::new(move |level, message, at| {
        let level = match level {
            rustkit_js::LogLevel::Log => ConsoleLevel::Log,
            rustkit_js::LogLevel::Info => ConsoleLevel::Info,
            rustkit_js::LogLevel::Warn => ConsoleLevel::Warn,
            rustkit_js::LogLevel::Error => ConsoleLevel::Error,
            rustkit_js::LogLevel::Debug => ConsoleLevel::Debug,
        };
        // Boa places a call at its `(`; a browser places it at the method's
        // name (`console.log(` is `log`), which is the name's length before.
        let method_len = match level {
            ConsoleLevel::Log => 3,
            ConsoleLevel::Info | ConsoleLevel::Warn => 4,
            ConsoleLevel::Error | ConsoleLevel::Debug => 5,
        };
        let location = at.and_then(|at| {
            let column = if at.column > method_len { at.column - method_len } else { at.column };
            sink.frame_location(&at.path, at.line, column)
        });
        sink.console(level, message, location);
    }));
    let _ = bindings.set_error_details(true, inspector.keeps_error_text());
}

/// Push `record` onto a script log. A page exception in it reaches the
/// Inspector from the runtime's report at the next drain; only the engine's
/// own failures (a panic inside the JS engine, an import map it refused),
/// which the engine marks where it raises them, are recorded from here,
/// with a fixed message. What page script threw is never one of them,
/// whatever its text.
pub(crate) fn log_script(log: &mut Vec<ScriptRecord>, inspector: &Inspector, record: ScriptRecord) {
    if let ScriptOutcome::EngineFailed(failure) = &record.outcome {
        engine_error(inspector, failure, &record.source);
    }
    log.push(record);
}

/// The engine's own `failure` as a `script_error` with a fixed message.
pub(crate) fn engine_error(inspector: &Inspector, failure: &EngineFailure, script: &str) {
    let (message, length) = match failure {
        EngineFailure::Panic => ("JS engine panic", None),
        EngineFailure::ImportMap(why) => ("import map rejected", Some(why.chars().count() as u64)),
    };
    inspector.emit_with(|inspector| InspectorEvent::ScriptError {
        name: String::new(),
        message: Some(message.to_string()),
        location: None,
        stack: None,
        unhandled_rejection: false,
        script: inspector.url_text(script),
        value: "engine".to_string(),
        message_len: length,
    });
}

/// One page error, from the runtime's report: under `script` unless it is a
/// module's own late error.
pub(crate) fn script_error(inspector: &Inspector, report: &rustkit_js::ErrorReport, script: &str) {
    inspector.emit_with(|inspector| {
        let frames: Vec<SourceLocation> = report
            .frames
            .iter()
            .filter_map(|f| inspector.frame_location(&f.path, f.line, f.column))
            .collect();
        let location = frames.first().filter(|_| report.located).cloned();
        let stack = (!frames.is_empty()).then(|| {
            let lines: Vec<String> = frames
                .iter()
                .map(|f| format!("    at {}:{}:{}", f.url, f.line, f.column))
                .collect();
            cap(&lines.join("\n"), MAX_STACK_CHARS).0
        });
        InspectorEvent::ScriptError {
            name: report.name.unwrap_or("").to_string(),
            message: report.message.as_deref().map(|m| cap(m, MAX_MESSAGE_CHARS).0),
            location,
            stack,
            unhandled_rejection: report.rejection,
            script: inspector.url_text(report.module.as_deref().unwrap_or(script)),
            value: report.value.to_string(),
            message_len: report.message_len.map(|n| n as u64),
        }
    });
}

/// After `source` ran: the errors its listeners and timers reported go onto
/// the log (as before), then, for a listening Inspector only, the page's
/// error reports and unimplemented members.
pub(crate) fn drain_reports(bindings: &DomBindings, inspector: &Inspector, log: &mut Vec<ScriptRecord>, source: &str) {
    for message in bindings.take_reported_errors() {
        log.push(ScriptRecord {
            source: source.to_string(),
            bytes: 0,
            elapsed_ms: 0,
            outcome: ScriptOutcome::Threw(message),
        });
    }
    drain_quiet(bindings, inspector, source);
}

/// After the page's script `script` ran: what its listeners and callbacks
/// reported (a synchronous `click()`, a custom element reaction) goes to the
/// Inspector now, as this script's, and onto `carried` for the script log,
/// which records it under the lifecycle step that drains next, as it always
/// has: a script whose listener threw is not a script that threw.
pub(crate) fn drain_script_reports(bindings: &DomBindings, inspector: &Inspector, carried: &mut Vec<String>, script: &str) {
    carried.extend(bindings.take_reported_errors());
    drain_quiet(bindings, inspector, script);
}

/// The page's error reports and unimplemented members since the last
/// drain: Inspector events only, never script log records (the board's
/// counts stay what they were).
pub(crate) fn drain_quiet(bindings: &DomBindings, inspector: &Inspector, source: &str) {
    if !inspector.is_active() {
        return;
    }
    drain_errors(bindings, inspector, source);
    for (interface, member, count, at) in bindings.take_unsupported_apis() {
        let location = at.and_then(|at| inspector.frame_location(&at.path, at.line, at.column));
        inspector.unsupported_api(&interface, &member, count, location);
    }
}

/// The page's error reports since the last drain, under `source`.
pub(crate) fn drain_errors(bindings: &DomBindings, inspector: &Inspector, source: &str) {
    if !inspector.is_active() {
        return;
    }
    for report in bindings.take_error_reports() {
        script_error(inspector, &report, source);
    }
}
