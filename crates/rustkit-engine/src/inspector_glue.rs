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
        let location = at.map(|at| {
            let column = if at.column > method_len { at.column - method_len } else { at.column };
            sink.location(&at.path, at.line, column)
        });
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
        // captured beside it does.
        let stack = stack.or(own_stack);
        // Where it was thrown: the innermost frame that is the page's.
        // Code in no named script (eval, an `on*` attribute) has no place
        // of its own, and is never given its caller's. With no stack, the
        // position Boa put on the message line.
        let position = match stack.as_deref() {
            Some(stack) => throw_site(stack),
            None => position.filter(|(path, _, _)| !path.is_empty() && !path.contains("eval at")),
        };
        // Only the page's frames are kept.
        let stack = stack.map(|s| page_frames(&s)).filter(|s| !s.is_empty());
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

/// After the page's script `script` ran: what its listeners and callbacks
/// reported (a synchronous `click()`, a custom element reaction) goes to the
/// Inspector now, as this script's, and onto `carried` for the script log,
/// which records it under the lifecycle step that drains next, as it always
/// has: a script whose listener threw is not a script that threw.
pub(crate) fn drain_script_reports(bindings: &DomBindings, inspector: &Inspector, carried: &mut Vec<String>, script: &str) {
    for (message, stack) in bindings.take_reported_error_details() {
        script_error(inspector, &message, stack, false, script);
        carried.push(message);
    }
    drain_quiet(bindings, inspector, script);
}

/// Late module errors, unhandled rejections and unimplemented members
/// since the last drain:
/// Inspector events only, never script log records (the board's counts
/// stay what they were).
pub(crate) fn drain_quiet(bindings: &DomBindings, inspector: &Inspector, source: &str) {
    if !inspector.is_active() {
        return;
    }
    // A module whose top-level `await` outlived its script step and then
    // threw: that module's uncaught error, as Chromium reports it.
    for (module, text) in bindings.take_late_module_errors() {
        script_error(inspector, &text, None, false, &module);
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
/// functions, the engine's own (unnamed) glue scripts, and `eval` code
/// (Boa names no script for it; the frame that called `eval` stays). Only
/// frame lines (`at name (path:line:col)`) are kept: anything else in the
/// text is not a frame.
fn page_frames(stack: &str) -> String {
    stack
        .lines()
        .filter(|line| {
            let line = line.trim_end();
            let frame = line.trim_start().starts_with("at ") && line.ends_with(')');
            frame
                && !(line.ends_with("(native)")
                    || line.contains("(unknown at ")
                    || line.contains(" (:")
                    || line.contains("eval at"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The innermost page frame of a Boa stack, passing over native functions
/// and the engine's own unnamed glue; `None` when the innermost page code is
/// in no named script: `eval` code (`<eval>`, `eval at`) or a function built
/// from a string (`anonymous`: an `on*` attribute's handler).
fn throw_site(stack: &str) -> Option<crate::inspector::PositionParts> {
    for line in stack.lines() {
        let line = line.trim();
        if line.is_empty() || line.ends_with("(native)") {
            continue;
        }
        if line.contains("eval at") || line.starts_with("at <eval>") || line.starts_with("at anonymous (unknown at ") {
            return None;
        }
        if line.contains("(unknown at ") || line.contains(" (:") {
            continue;
        }
        return parse_error_text(&format!("x\n{line}")).2.filter(|(path, _, _)| !path.is_empty());
    }
    None
}
