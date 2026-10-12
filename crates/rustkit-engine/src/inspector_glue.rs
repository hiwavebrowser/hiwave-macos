//! Where the page's script reaches the Inspector: the console handler, the
//! exceptions the script log records, unhandled rejections and the
//! unimplemented members the bindings saw. Every function here returns at
//! once when the view's Inspector has nobody listening.

use std::sync::Arc;

use rustkit_bindings::DomBindings;

use crate::inspector::{cap, parse_error_text, ConsoleLevel, Inspector, InspectorEvent, MAX_SHORT_CHARS, MAX_STACK_CHARS};
use crate::{ScriptOutcome, ScriptRecord};

/// Set up a document's new bindings for a listening Inspector: console
/// calls with their positions, rejection tracking, listener error stacks.
/// Nothing is set up (and nothing costs anything) when nobody listens.
pub(crate) fn attach(bindings: &DomBindings, inspector: &Arc<Inspector>) {
    if !inspector.is_active() {
        return;
    }
    let sink = inspector.clone();
    bindings.set_console_handler(Box::new(move |level, message, at| {
        let level = match level {
            rustkit_js::LogLevel::Log => ConsoleLevel::Log,
            rustkit_js::LogLevel::Info => ConsoleLevel::Info,
            rustkit_js::LogLevel::Warn => ConsoleLevel::Warn,
            rustkit_js::LogLevel::Error => ConsoleLevel::Error,
            rustkit_js::LogLevel::Debug => ConsoleLevel::Debug,
        };
        let location = at.map(|at| sink.location(&at.path, at.line, at.column));
        sink.console(level, message, location);
    }));
    let _ = bindings.set_error_details(true);
}

/// Push `record` onto a script log; an exception in it is also a
/// `ScriptError` (with `stack` when the caller has one apart from the text).
pub(crate) fn log_script(log: &mut Vec<ScriptRecord>, inspector: &Inspector, record: ScriptRecord, stack: Option<String>) {
    if let ScriptOutcome::Threw(text) = &record.outcome {
        script_error(inspector, text, stack, false, &record.source);
    }
    log.push(record);
}

/// Record one exception, `text` as the script log keeps it.
pub(crate) fn script_error(inspector: &Inspector, text: &str, stack: Option<String>, rejection: bool, script: &str) {
    inspector.emit_with(|inspector| {
        let (name, message, position, own_stack) = parse_error_text(text);
        // A listener's error text has no stack of its own; the one
        // captured beside it does. Only the page's frames are kept.
        let stack = stack.or(own_stack).map(|s| page_frames(&s)).filter(|s| !s.is_empty());
        // Where it was thrown: the innermost page frame, else the
        // position Boa put on the message line.
        let position = stack
            .as_deref()
            .and_then(|s| parse_error_text(&format!("x\n{s}")).2)
            .or(position)
            .filter(|(path, _, _)| !path.is_empty());
        let location = position.map(|(path, line, column)| inspector.location(&path, line, column));
        InspectorEvent::ScriptError {
            name: cap(&name, MAX_SHORT_CHARS).0,
            message: inspector.scrub_text(&cap(&message, crate::inspector::MAX_MESSAGE_CHARS).0),
            location,
            stack: stack.map(|s| inspector.scrub_text(&cap(&s, MAX_STACK_CHARS).0)),
            unhandled_rejection: rejection,
            script: inspector.url_text(script),
        }
    });
}

/// After `source` ran: the errors its listeners and timers reported (onto
/// the log, as before, and to the Inspector with their stacks), then, for a
/// listening Inspector only, unhandled rejections and unimplemented members.
pub(crate) fn drain_reports(bindings: &DomBindings, inspector: &Inspector, log: &mut Vec<ScriptRecord>, source: &str) {
    for (message, stack) in bindings.take_reported_error_details() {
        let record = ScriptRecord {
            source: source.to_string(),
            bytes: 0,
            elapsed_ms: 0,
            outcome: ScriptOutcome::Threw(message),
        };
        log_script(log, inspector, record, stack);
    }
    drain_quiet(bindings, inspector, source);
}

/// Unhandled rejections and unimplemented members since the last drain:
/// Inspector events only, never script log records (the board's counts
/// stay what they were).
pub(crate) fn drain_quiet(bindings: &DomBindings, inspector: &Inspector, source: &str) {
    if !inspector.is_active() {
        return;
    }
    for rejection in bindings.take_unhandled_rejections() {
        let text = match &rejection.stack {
            Some(stack) => format!("{}\n{stack}", rejection.message),
            None => rejection.message.clone(),
        };
        script_error(inspector, &text, None, true, source);
    }
    for (interface, member, count, at) in bindings.take_unsupported_apis() {
        let location = at.map(|at| inspector.location(&at.path, at.line, at.column));
        inspector.unsupported_api(&interface, &member, count, location);
    }
}

/// A Boa stack without the frames that are not the page's: native
/// functions and the engine's own (unnamed) glue scripts.
fn page_frames(stack: &str) -> String {
    stack
        .lines()
        .filter(|line| {
            let line = line.trim_end();
            !(line.ends_with("(native)") || line.contains("(unknown at ") || line.contains(" (:") || line.trim().is_empty())
        })
        .collect::<Vec<_>>()
        .join("\n")
}
