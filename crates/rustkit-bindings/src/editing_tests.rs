//! CH2.E1: `document.execCommand`, `navigator.clipboard`, and the
//! `beforeinput`/`input` a user's edit command fires. The commands run on
//! rustkit-dom's TextEditState, the model the engine's keys drive.

use super::*;
use rustkit_dom::clipboard::{Clipboard, MemoryClipboard};
use rustkit_dom::forms::EditCommand;

const PAGE: &str = r#"<!DOCTYPE html><html><body>
<input id="q" value="hello"><input id="pw" type="password" value="secret">
<input id="short" maxlength="4"><textarea id="t">a</textarea>
<input id="box" type="checkbox"><div id="plain">text</div>
</body></html>"#;

struct Page {
    b: DomBindings,
    doc: Rc<Document>,
    cb: Rc<MemoryClipboard>,
}

impl Page {
    fn id(&self, el: &str) -> usize {
        self.doc.get_element_by_id(el).unwrap().id.raw()
    }
}

fn page() -> Page {
    let b = DomBindings::new(JsRuntime::new().unwrap()).unwrap();
    let doc = Rc::new(Document::parse_html(PAGE).unwrap());
    b.set_document(doc.clone()).unwrap();
    let cb = Rc::new(MemoryClipboard::new());
    b.set_clipboard(cb.clone());
    b.evaluate(
        "var $ = function (i) { return document.getElementById(i); }; window.log = []; \
         ['beforeinput', 'input'].forEach(function (t) { document.addEventListener(t, function (e) { \
         log.push(t + ':' + e.target.id + ':' + e.inputType + ':' + e.data + ':' + e.target.value); }); });",
    )
    .unwrap();
    Page { b, doc, cb }
}

fn ev(b: &DomBindings, script: &str) -> String {
    match b.evaluate(script).unwrap() {
        JsValue::String(s) => s,
        JsValue::Boolean(v) => v.to_string(),
        JsValue::Number(n) => n.to_string(),
        JsValue::Null => "null".to_string(),
        JsValue::Undefined => "undefined".to_string(),
        other => panic!("{script} evaluated to {other:?}"),
    }
}

#[test]
fn exec_command_copy_with_a_selection_fills_the_clipboard() {
    let p = page();
    assert_eq!(
        ev(
            &p.b,
            "$('q').focus(); $('q').setSelectionRange(1, 4); document.execCommand('copy')"
        ),
        "true"
    );
    assert_eq!(p.cb.read_text().as_deref(), Some("ell"));
    // Copy changes nothing, so no input event.
    assert_eq!(ev(&p.b, "$('q').value + '|' + log.length"), "hello|0");
}

#[test]
fn navigator_clipboard_write_then_read_resolves_the_text() {
    let p = page();
    p.b.evaluate(
        "window.got = []; navigator.clipboard.writeText('x').then(function (v) { got.push('w:' + v); \
         return navigator.clipboard.readText(); }).then(function (t) { got.push('r:' + t); });",
    )
    .unwrap();
    p.b.run_jobs().unwrap();
    assert_eq!(ev(&p.b, "got.join()"), "w:undefined,r:x");
    assert_eq!(p.cb.read_text().as_deref(), Some("x"));
    assert_eq!(
        ev(
            &p.b,
            "navigator.clipboard instanceof Clipboard && typeof navigator.clipboard.readText"
        ),
        "function"
    );
}

#[test]
fn a_page_denied_clipboard_reads_gets_a_rejection() {
    let p = page();
    p.cb.write_text("private");
    p.b.set_clipboard_read_allowed(false);
    p.b.evaluate(
        "window.got = []; navigator.clipboard.readText().then(function (t) { got.push('r:' + t); }, \
         function (e) { got.push(e.name); });",
    )
    .unwrap();
    p.b.run_jobs().unwrap();
    assert_eq!(ev(&p.b, "got.join()"), "NotAllowedError");
    assert_eq!(
        ev(&p.b, "$('q').focus(); document.execCommand('paste')"),
        "false"
    );
    assert_eq!(ev(&p.b, "$('q').value"), "hello");
}

#[test]
fn exec_command_edits_the_focused_control_and_fires_input() {
    let p = page();
    assert_eq!(
        ev(&p.b, "$('q').focus(); document.execCommand('selectAll')"),
        "true"
    );
    assert_eq!(
        ev(&p.b, "$('q').selectionStart + ',' + $('q').selectionEnd"),
        "0,5"
    );
    assert_eq!(ev(&p.b, "document.execCommand('cut')"), "true");
    assert_eq!(p.cb.read_text().as_deref(), Some("hello"));
    assert_eq!(ev(&p.b, "$('q').value"), "");
    assert_eq!(
        ev(&p.b, "document.execCommand('insertText', false, 'ab')"),
        "true"
    );
    assert_eq!(ev(&p.b, "document.execCommand('paste')"), "true");
    assert_eq!(
        ev(&p.b, "$('q').value + '|' + $('q').selectionStart"),
        "abhello|7"
    );
    assert_eq!(ev(&p.b, "document.execCommand('undo')"), "true");
    assert_eq!(ev(&p.b, "$('q').value"), "ab");
    assert_eq!(ev(&p.b, "document.execCommand('redo')"), "true");
    assert_eq!(ev(&p.b, "$('q').value"), "abhello");
    // execCommand fires `input` (not `beforeinput`, Input Events §5.1).
    assert_eq!(
        ev(&p.b, "log.join(' / ')"),
        "input:q:deleteByCut:null: / input:q:insertText:ab:ab / input:q:insertFromPaste:hello:abhello / \
         input:q:historyUndo:null:ab / input:q:historyRedo:null:abhello"
    );
}

#[test]
fn exec_command_answers_false_when_it_cannot_run() {
    let p = page();
    // Nothing focused.
    assert_eq!(ev(&p.b, "document.execCommand('selectAll')"), "false");
    // Unsupported commands.
    assert_eq!(
        ev(&p.b, "$('q').focus(); document.execCommand('bold')"),
        "false"
    );
    // Nothing to undo yet.
    assert_eq!(ev(&p.b, "document.execCommand('undo')"), "false");
    // A password field refuses copy.
    p.cb.write_text("keep");
    assert_eq!(
        ev(
            &p.b,
            "$('pw').focus(); $('pw').select(); document.execCommand('copy')"
        ),
        "false"
    );
    assert_eq!(p.cb.read_text().as_deref(), Some("keep"));
    // A checkbox is not a text control.
    assert_eq!(
        ev(&p.b, "$('box').focus(); document.execCommand('paste')"),
        "false"
    );
    assert_eq!(
        ev(&p.b, "['copy','cut','paste','selectAll','undo','redo','insertText','bold'].map(function (c) { \
                  return document.queryCommandSupported(c); }).join()"),
        "true,true,true,true,true,true,true,false"
    );
}

#[test]
fn exec_command_paste_respects_maxlength() {
    let p = page();
    p.cb.write_text("123456");
    assert_eq!(
        ev(&p.b, "$('short').focus(); document.execCommand('paste')"),
        "true"
    );
    assert_eq!(ev(&p.b, "$('short').value"), "1234");
}

#[test]
fn a_user_paste_fires_beforeinput_with_insert_from_paste_then_input() {
    let p = page();
    p.cb.write_text("XY");
    let q = p.id("q");
    let out = p.b.user_edit(q, &EditCommand::Paste).unwrap();
    assert!(out.changed);
    assert_eq!(out.input_type, Some("insertFromPaste"));
    assert_eq!(
        ev(&p.b, "log.join(' / ')"),
        "beforeinput:q:insertFromPaste:XY:hello / input:q:insertFromPaste:XY:helloXY"
    );
    assert_eq!(
        ev(&p.b, "$('q').value + '|' + $('q').selectionStart"),
        "helloXY|7"
    );
}

#[test]
fn a_cancelled_beforeinput_stops_the_edit() {
    let p = page();
    p.cb.write_text("XY");
    p.b.evaluate("$('q').addEventListener('beforeinput', function (e) { e.preventDefault(); });")
        .unwrap();
    let out = p.b.user_edit(p.id("q"), &EditCommand::Paste).unwrap();
    assert!(!out.changed);
    assert_eq!(ev(&p.b, "$('q').value + '|' + log.length"), "hello|1");
}

#[test]
fn user_cut_and_undo_fire_their_input_types() {
    let p = page();
    let q = p.id("q");
    p.b.user_edit(q, &EditCommand::SelectAll).unwrap();
    p.b.user_edit(q, &EditCommand::Cut).unwrap();
    p.b.user_edit(q, &EditCommand::Undo).unwrap();
    assert_eq!(
        ev(&p.b, "log.join(' / ')"),
        "beforeinput:q:deleteByCut:null:hello / input:q:deleteByCut:null: / \
         beforeinput:q:historyUndo:null: / input:q:historyUndo:null:hello"
    );
    assert_eq!(p.cb.read_text().as_deref(), Some("hello"));
}

#[test]
fn edits_run_on_the_shared_edit_state_the_engine_paints() {
    let p = page();
    let states: SharedEditStates = Rc::default();
    let t = p.id("t");
    let typed = rustkit_dom::forms::TextEditState::with_value("a");
    typed.insert_text("b");
    states.borrow_mut().insert(t, typed);
    p.b.share_edit_states(states.clone());
    p.b.sync_control_value(t, "ab".to_string());
    // Script's undo undoes the user's typing, in the engine's state.
    assert_eq!(
        ev(&p.b, "$('t').focus(); document.execCommand('undo')"),
        "true"
    );
    assert_eq!(states.borrow()[&t].value(), "a");
    assert_eq!(ev(&p.b, "$('t').value"), "a");
    // A textarea's paste keeps line breaks.
    p.cb.write_text("1\n2");
    assert_eq!(ev(&p.b, "document.execCommand('paste')"), "true");
    assert_eq!(states.borrow()[&t].value(), "a1\n2");
}
