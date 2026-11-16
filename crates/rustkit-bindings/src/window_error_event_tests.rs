//! The window's `error` event for an uncaught exception (HTML "report an exception").
//!
//! The expected answers are the pinned Chrome's (CfT 148.0.7778.216, headless, recorded
//! 2026-11-16) on `docs/diagnostics/2026-11-16/window_error{.html,_page.js}`, in
//! `window_error_cft148.json`; the engine's end-to-end test runs that page
//! (`rustkit-engine` `window_error_event_tests`). Here, each rule alone:
//!   - an uncaught exception fires one trusted, cancelable, non-bubbling `ErrorEvent` at the
//!     Window, at once, with `message` ("Uncaught " and the error), `filename`, `lineno`,
//!     `colno` and `error`;
//!   - `window.onerror` is called with those five and `this` the Window, and `true` back
//!     cancels the event; a listener's `preventDefault()` cancels it too;
//!   - a canceled error is not reported any further (the engine's log), any other once;
//!   - an exception thrown by an `error` handler is reported and fires no second event;
//!   - a promise job's exception is an `unhandledrejection`, never an `error`;
//!   - `reportError(e)` does the same for `e`, and with no argument is a `TypeError`.

use super::*;

const WATCH: &str = "var seen = [], calls = [], rejected = 0; \
    window.addEventListener('error', function (ev) { \
        seen.push([Object.prototype.toString.call(ev), ev.message, ev.filename, ev.lineno, ev.colno, \
                   ev.error instanceof Error ? ev.error.name : String(ev.error), ev.isTrusted, ev.cancelable, ev.bubbles, ev.target === window].join('|')); }); \
    window.addEventListener('unhandledrejection', function () { rejected++; });";

fn page() -> DomBindings {
    let b = DomBindings::new(JsRuntime::new().unwrap()).unwrap();
    b.set_document(Rc::new(Document::parse_html("<html><head></head><body><button id=b>x</button></body></html>").unwrap()))
        .unwrap();
    b.set_location(&Url::parse("https://example.test/page.html").unwrap()).unwrap();
    b.evaluate_named(WATCH, "inline#1").unwrap();
    b
}

fn ev(b: &DomBindings, script: &str) -> String {
    match b.evaluate(script).unwrap() {
        JsValue::String(s) => s,
        other => format!("{other:?}"),
    }
}

/// Chrome rows `top level`, `listener` (two events by the time `dispatchEvent` returns:
/// `syncAfterDispatch`), `timer`, `frame callback`, `inline handler attribute`.
#[test]
fn an_uncaught_exception_fires_one_error_event_at_the_window() {
    let b = page();
    assert!(b.evaluate_named("throw new Error('from a script');", "https://example.test/a.js").is_err());
    assert_eq!(ev(&b, "seen.join(' ; ')"), "[object ErrorEvent]|Uncaught Error: from a script|https://example.test/a.js|1|7|Error|true|true|false|true");
    b.evaluate_named(
        "seen.length = 0; var btn = document.getElementById('b'); \
         btn.addEventListener('click', function () { throw new TypeError('from a listener'); }); \
         btn.dispatchEvent(new Event('click')); var atOnce = seen.length; \
         setTimeout(function () { throw new RangeError('from a timer'); }, 0); \
         requestAnimationFrame(function () { throw 'a string'; }); \
         btn.setAttribute('onkeydown', 'throw null'); btn.dispatchEvent(new Event('keydown'));",
        "inline#2",
    )
    .unwrap();
    b.advance_timers(20, 10).unwrap();
    assert_eq!(ev(&b, "String(atOnce)"), "1");
    let seen = ev(&b, "seen.map(function (s) { var f = s.split('|'); return [f[1], f[2], f[5]].join('|'); }).join(' ; ')");
    assert_eq!(
        seen,
        "Uncaught TypeError: from a listener|https://example.test/page.html|TypeError ; \
         Uncaught null|https://example.test/page.html|null ; \
         Uncaught RangeError: from a timer|https://example.test/page.html|RangeError ; \
         Uncaught a string|https://example.test/page.html|a string"
    );
    // Each is still reported once.
    assert_eq!(b.take_reported_errors().len(), 4);
}

/// Chrome rows `onerror returns true` (`cancelSeen`), `preventDefault` (`preventSeen`) and the
/// `onerror` list (five arguments, `this` the Window; the handler before a listener added after it).
#[test]
fn onerror_gets_five_arguments_and_true_back_cancels() {
    let b = page();
    b.evaluate_named(
        "var order = []; \
         window.onerror = function (message, source, lineno, colno, error) { \
             order.push('onerror'); \
             calls.push([arguments.length, message, source, typeof lineno, typeof colno, error && error.name, this === window].join('|')); \
             return message.indexOf('cancel me') >= 0; }; \
         var later = []; \
         window.addEventListener('error', function (ev) { order.push('listener'); later.push(ev.defaultPrevented + '/' + ev.returnValue); \
             if (ev.message.indexOf('prevent me') >= 0) ev.preventDefault(); }); \
         setTimeout(function () { throw new Error('cancel me'); }, 0); \
         setTimeout(function () { throw new Error('prevent me'); }, 1); \
         setTimeout(function () { throw new Error('report me'); }, 2);",
        "inline#2",
    )
    .unwrap();
    b.advance_timers(20, 10).unwrap();
    assert_eq!(
        ev(&b, "calls.join(' ; ')"),
        "5|Uncaught Error: cancel me|https://example.test/page.html|number|number|Error|true ; \
         5|Uncaught Error: prevent me|https://example.test/page.html|number|number|Error|true ; \
         5|Uncaught Error: report me|https://example.test/page.html|number|number|Error|true"
    );
    assert_eq!(ev(&b, "later.join() + ' ' + order.slice(0, 2).join()"), "true/false,false/true,false/true onerror,listener");
    assert_eq!(b.take_reported_errors(), vec!["Error: report me".to_string()], "a canceled error is not reported further");
}

/// Chrome `inHandler`: one call. The handler's own exception is reported and fires nothing.
#[test]
fn an_exception_in_an_error_handler_fires_no_second_event() {
    let b = page();
    b.evaluate_named(
        "var handled = 0; window.addEventListener('error', function () { handled++; throw new Error('from the handler'); }); \
         window.onerror = function () { handled += 10; throw new Error('from onerror'); }; \
         setTimeout(function () { throw new Error('first'); }, 0);",
        "inline#2",
    )
    .unwrap();
    b.advance_timers(20, 10).unwrap();
    assert_eq!(ev(&b, "handled + ' ' + seen.length"), "11 1");
    let mut reported = b.take_reported_errors();
    reported.sort();
    assert_eq!(reported, vec!["Error: first", "Error: from onerror", "Error: from the handler"]);
    // The next uncaught exception fires as usual.
    b.evaluate_named("setTimeout(function () { throw new Error('second'); }, 0);", "inline#3").unwrap();
    b.advance_timers(20, 10).unwrap();
    assert_eq!(ev(&b, "handled + ' ' + seen.length"), "22 2");
}

/// Chrome `rejections` and no `promise job` row among the events.
#[test]
fn a_promise_jobs_exception_is_a_rejection_not_an_error() {
    let b = page();
    b.evaluate_named("Promise.resolve().then(function () { throw new Error('from a promise job'); });", "inline#2").unwrap();
    b.advance_timers(20, 10).unwrap();
    assert_eq!(ev(&b, "seen.length + ' ' + rejected"), "0 1");
}

/// Chrome `reportError`, `reportErrorSync`, `reportErrorSame`, `reportErrorNoArg`.
#[test]
fn report_error_reports_its_argument() {
    let b = page();
    b.evaluate_named(
        "var e = new RangeError('reported'), last; window.addEventListener('error', function (ev) { last = ev.error; }); \
         var kind = typeof self.reportError; reportError(e); var atOnce = seen.length, same = last === e, bare; \
         try { reportError(); bare = 'returned'; } catch (x) { bare = x.name; }",
        "inline#2",
    )
    .unwrap();
    assert_eq!(ev(&b, "[kind, atOnce, same, bare, seen[0].split('|')[1]].join()"), "function,1,true,TypeError,Uncaught RangeError: reported");
}

/// Chrome `pageDispatchCallsOnerror`: an `ErrorEvent` the page dispatches at the Window calls
/// `onerror` with the event's five fields too; any other event calls it with the event.
#[test]
fn onerror_takes_a_page_dispatched_error_events_fields() {
    let b = page();
    b.evaluate_named(
        "window.onerror = function () { calls.push(arguments.length + ':' + Array.prototype.join.call(arguments, '|')); }; \
         window.dispatchEvent(new ErrorEvent('error', { message: 'made by the page', filename: 'f.js', lineno: 3, colno: 4, error: null })); \
         window.dispatchEvent(new Event('error'));",
        "inline#2",
    )
    .unwrap();
    assert_eq!(ev(&b, "calls.join(' ; ')"), "5:made by the page|f.js|3|4| ; 1:[object Event]");
}

/// The event is the engine's own whatever the page has replaced: the constructors, the
/// property helpers and the call helpers of the page are not asked.
#[test]
fn what_the_page_replaced_does_not_change_the_event() {
    let b = page();
    b.evaluate_named(
        "var real = { toString: Object.prototype.toString, proto: ErrorEvent.prototype }, ran = 0, got = []; \
         window.addEventListener('error', function (ev) { \
             got.push(real.toString.call(ev) + '|' + (Object.getPrototypeOf(ev) === real.proto) + '|' + ev.message + '|' + ev.isTrusted); }); \
         function no() { ran++; throw new Error('the page was asked'); } \
         window.ErrorEvent = no; window.Event = no; \
         Object.defineProperty = no; Object.create = no; Object.setPrototypeOf = no; \
         Function.prototype.call = no; Function.prototype.apply = no; Function.prototype.bind = no; \
         String.prototype.indexOf = no; String.prototype.slice = no; RegExp.prototype.exec = no; \
         ran = 0; setTimeout(function () { throw new Error('still an event'); }, 0);",
        "inline#2",
    )
    .unwrap();
    b.advance_timers(20, 10).unwrap();
    let b2 = &b;
    assert!(matches!(b2.evaluate("ran"), Ok(JsValue::Number(n)) if n == 0.0), "page functions ran");
    assert!(matches!(b2.evaluate("got[0]"), Ok(JsValue::String(s)) if s == "[object ErrorEvent]|true|Uncaught Error: still an event|true"));
}
