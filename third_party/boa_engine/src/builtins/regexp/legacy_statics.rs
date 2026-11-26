//! HIWAVE PATCH (REGEXP-LEGACY-STATICS): the legacy RegExp static properties
//! (`RegExp.input`/`$_`, `lastMatch`/`$&`, `lastParen`/`$+`, `leftContext`/`` $` ``,
//! `rightContext`/`$'`, `$1`..`$9`) from the TC39 "RegExp legacy features" proposal, which
//! every shipping engine implements and real code reads: JetStream 2.2's WSL lexer does
//! `this._index += RegExp.lastMatch.length` after each `test`, so without them its first test
//! throws "cannot convert 'null' or 'undefined' to object".
//!
//! State belongs to a realm (kept in the realm's host-defined slot): a match updates the statics of
//! the realm that is current when it runs, and the getters read their own realm's. Frames run in
//! their own realms on one thread, so nothing one realm matched is visible through another's
//! `RegExp`. The proposal's subclass rule is not applied yet (stated in PATCHES.md).

use crate::{Context, JsData, JsResult, JsString, JsValue, js_string};
use boa_gc::{Finalize, Trace};
use regress::Range;

#[derive(Default, Trace, Finalize, JsData)]
struct LegacyState {
    /// What `RegExp.input` reads; the setter changes only this.
    #[unsafe_ignore_trace]
    input: Option<JsString>,
    /// The string the last match was taken from; the match and context getters slice it.
    #[unsafe_ignore_trace]
    subject: Option<JsString>,
    #[unsafe_ignore_trace]
    matched: Option<Range>,
    #[unsafe_ignore_trace]
    captures: Vec<Option<Range>>,
}

/// Run `f` on the current realm's state, creating it on first use.
fn with_state<R>(context: &mut Context, f: impl FnOnce(&mut LegacyState) -> R) -> R {
    let realm = context.realm().clone();
    let mut host = realm.host_defined_mut();
    if host.get::<LegacyState>().is_none() {
        host.insert(LegacyState::default());
    }
    f(host.get_mut::<LegacyState>().expect("inserted above"))
}

/// The other statics have a setter too in Chrome 148 (measured), which changes nothing.
pub(super) fn set_ignored(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    Ok(JsValue::undefined())
}

/// Record a successful match (called from `abstract_builtin_exec`).
pub(super) fn record(input: &JsString, matched: Range, captures: &[Option<Range>], context: &mut Context) {
    with_state(context, |s| {
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
fn read(_this: &JsValue, context: &mut Context, f: impl FnOnce(&LegacyState, &JsString) -> JsValue) -> JsResult<JsValue> {
    Ok(with_state(context, |s| match s.subject.clone() {
        Some(subject) => f(s, &subject),
        None => JsValue::new(js_string!()),
    }))
}

pub(super) fn get_input(_this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    Ok(with_state(context, |s| JsValue::new(s.input.clone().unwrap_or_default())))
}

pub(super) fn set_input(_this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let value = args.first().cloned().unwrap_or_default().to_string(context)?;
    with_state(context, |s| s.input = Some(value));
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
