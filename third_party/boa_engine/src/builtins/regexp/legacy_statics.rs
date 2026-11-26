//! HIWAVE PATCH (REGEXP-LEGACY-STATICS): the legacy RegExp static properties
//! (`RegExp.input`/`$_`, `lastMatch`/`$&`, `lastParen`/`$+`, `leftContext`/`` $` ``,
//! `rightContext`/`$'`, `$1`..`$9`) from the TC39 "RegExp legacy features" proposal, which
//! every shipping engine implements and real code reads: JetStream 2.2's WSL lexer does
//! `this._index += RegExp.lastMatch.length` after each `test`, so without them its first test
//! throws "cannot convert 'null' or 'undefined' to object".
//!
//! State is the last successful RegExpBuiltinExec on this thread. The proposal keeps it per
//! realm and skips the update for subclass instances and cross-realm calls; neither is done
//! here yet (stated gap, see PATCHES.md).

use crate::{Context, JsResult, JsString, JsValue, js_string};
use regress::Range;
use std::cell::RefCell;

#[derive(Default)]
struct LegacyState {
    /// What `RegExp.input` reads; the setter changes only this.
    input: Option<JsString>,
    /// The string the last match was taken from; the match and context getters slice it.
    subject: Option<JsString>,
    matched: Option<Range>,
    captures: Vec<Option<Range>>,
}

thread_local! {
    static STATE: RefCell<LegacyState> = RefCell::new(LegacyState::default());
}

/// The other statics have a setter too in Chrome 148 (measured), which changes nothing.
pub(super) fn set_ignored(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    Ok(JsValue::undefined())
}

/// Record a successful match (called from `abstract_builtin_exec`).
pub(super) fn record(input: &JsString, matched: Range, captures: &[Option<Range>]) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.input = Some(input.clone());
        s.subject = Some(input.clone());
        s.matched = Some(matched);
        s.captures = captures.to_vec();
    });
}

fn slice(input: &JsString, range: &Range) -> JsValue {
    JsValue::new(JsString::from(input.get_expect(range.clone())))
}

// Chrome 148 answers whatever the this value (measured: `get.call({})` does not throw), so
// unlike the proposal there is no %RegExp% check.
fn read(_this: &JsValue, _context: &mut Context, f: impl FnOnce(&LegacyState, &JsString) -> JsValue) -> JsResult<JsValue> {
    Ok(STATE.with(|s| {
        let s = s.borrow();
        match s.subject.as_ref() {
            Some(subject) => f(&s, subject),
            None => JsValue::new(js_string!()),
        }
    }))
}

pub(super) fn get_input(_this: &JsValue, _: &[JsValue], _context: &mut Context) -> JsResult<JsValue> {
    Ok(STATE.with(|s| JsValue::new(s.borrow().input.clone().unwrap_or_default())))
}

pub(super) fn set_input(_this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let value = args.first().cloned().unwrap_or_default().to_string(context)?;
    STATE.with(|s| s.borrow_mut().input = Some(value));
    Ok(JsValue::undefined())
}

pub(super) fn get_last_match(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    read(this, context, |s, input| match &s.matched {
        Some(m) => slice(input, m),
        None => JsValue::new(js_string!()),
    })
}

pub(super) fn get_last_paren(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    read(this, context, |s, input| match s.captures.last() {
        Some(Some(r)) => slice(input, r),
        _ => JsValue::new(js_string!()),
    })
}

pub(super) fn get_left_context(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    read(this, context, |s, input| match &s.matched {
        Some(m) => slice(input, &(0..m.start)),
        None => JsValue::new(js_string!()),
    })
}

pub(super) fn get_right_context(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    read(this, context, |s, input| match &s.matched {
        Some(m) => slice(input, &(m.end..input.len())),
        None => JsValue::new(js_string!()),
    })
}

fn get_paren(n: usize, this: &JsValue, context: &mut Context) -> JsResult<JsValue> {
    read(this, context, |s, input| match s.captures.get(n - 1) {
        Some(Some(r)) => slice(input, r),
        _ => JsValue::new(js_string!()),
    })
}

macro_rules! paren_getters {
    ($($name:ident = $n:literal),*) => {
        $(pub(super) fn $name(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
            get_paren($n, this, context)
        })*
    };
}
paren_getters!(get_1 = 1, get_2 = 2, get_3 = 3, get_4 = 4, get_5 = 5, get_6 = 6, get_7 = 7, get_8 = 8, get_9 = 9);
