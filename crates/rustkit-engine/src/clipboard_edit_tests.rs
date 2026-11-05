//! CH2.E1: clipboard and edit commands in a focused text control, through
//! `Engine::handle_text_key` the way the window delivers keys (Cmd+C on
//! macOS arrives as `ctrl` + text "c", hiwave-app main.rs), and through
//! script (`execCommand`, `navigator.clipboard`).
//!
//! A page engine (no presenter, no GPU surface), as the content thread
//! builds it, so these run without a GPU adapter.

use super::*;

const PAGE: &str = concat!(
    r#"<html><body style="margin:0">"#,
    r#"<input id="q" value="hello"><input id="e"><input id="m" maxlength="5" value="abc">"#,
    r#"</body></html>"#,
);

const LEFT: u32 = 0x25;
const END: u32 = 0x23;

fn page() -> (Engine, EngineViewId) {
    let mut engine = Engine::new_page(EngineConfig::default(), None, Arc::new(AtomicU64::new(0)))
        .expect("engine");
    let id = engine
        .create_page_view(Bounds::new(0, 0, 800, 600))
        .expect("page view");
    engine.load_html(id, PAGE).expect("load_html");
    js(
        &mut engine,
        id,
        "window.log = []; var $ = function (i) { return document.getElementById(i); }; \
         ['beforeinput', 'input'].forEach(function (t) { document.addEventListener(t, function (e) { \
         log.push(t + ':' + e.inputType + ':' + e.data); }); }); 0",
    );
    (engine, id)
}

fn js(engine: &mut Engine, id: EngineViewId, script: &str) -> String {
    let out = engine
        .execute_script(id, &format!("String(eval({script:?}))"))
        .expect("script");
    // execute_script answers the value's Debug form: String("...").
    out.strip_prefix("String(\"")
        .and_then(|s| s.strip_suffix("\")"))
        .map(|s| s.replace("\\\"", "\""))
        .unwrap_or(out)
}

fn focus(engine: &mut Engine, id: EngineViewId, el: &str) {
    js(engine, id, &format!("$('{el}').focus()"));
    assert!(engine.focused_node(id).is_some(), "{el} focused");
}

/// The command key + a letter, as the window sends it.
fn chord(engine: &mut Engine, id: EngineViewId, letter: &str, shift: bool) -> bool {
    engine.handle_text_key(id, 0, letter, true, shift, false)
}

fn type_text(engine: &mut Engine, id: EngineViewId, text: &str) {
    for ch in text.chars() {
        assert!(engine.handle_text_key(id, 0, &ch.to_string(), false, false, false));
    }
}

fn value_of(engine: &mut Engine, id: EngineViewId, el: &str) -> String {
    let script_value = js(engine, id, &format!("$('{el}').value"));
    let raw = engine.views[&id]
        .document
        .as_ref()
        .unwrap()
        .get_element_by_id(el)
        .unwrap()
        .id
        .raw();
    let painted = engine.edit_value_in(id, raw).map(|v| v.0);
    assert_eq!(
        painted.as_deref(),
        Some(script_value.as_str()),
        "script and paint agree on {el}"
    );
    script_value
}

#[test]
fn select_all_copy_end_paste_doubles_hello() {
    let (mut engine, id) = page();
    focus(&mut engine, id, "q");
    assert!(chord(&mut engine, id, "a", false));
    chord(&mut engine, id, "c", false);
    assert_eq!(engine.clipboard().read_text().as_deref(), Some("hello"));
    assert!(engine.handle_text_key(id, END, "", false, false, false));
    assert!(chord(&mut engine, id, "v", false));
    assert_eq!(value_of(&mut engine, id, "q"), "hellohello");
}

#[test]
fn cut_leaves_the_field_empty_and_the_text_on_the_clipboard() {
    let (mut engine, id) = page();
    focus(&mut engine, id, "q");
    chord(&mut engine, id, "a", false);
    assert!(chord(&mut engine, id, "x", false));
    assert_eq!(value_of(&mut engine, id, "q"), "");
    assert_eq!(engine.clipboard().read_text().as_deref(), Some("hello"));
    assert_eq!(
        js(&mut engine, id, "log.join(' ')"),
        "beforeinput:deleteByCut:null input:deleteByCut:null"
    );
}

#[test]
fn undo_after_typing_restores_empty_and_redo_the_typing() {
    let (mut engine, id) = page();
    focus(&mut engine, id, "e");
    type_text(&mut engine, id, "abc");
    assert!(chord(&mut engine, id, "z", false));
    assert_eq!(value_of(&mut engine, id, "e"), "");
    assert!(chord(&mut engine, id, "z", true));
    assert_eq!(value_of(&mut engine, id, "e"), "abc");
}

#[test]
fn a_paste_past_maxlength_truncates() {
    let (mut engine, id) = page();
    engine.clipboard().write_text("12345");
    focus(&mut engine, id, "m");
    engine.handle_text_key(id, END, "", false, false, false);
    assert!(chord(&mut engine, id, "v", false));
    assert_eq!(value_of(&mut engine, id, "m"), "abc12");
}

#[test]
fn shift_left_twice_selects_two_chars_and_typing_replaces_them() {
    let (mut engine, id) = page();
    focus(&mut engine, id, "q");
    engine.handle_text_key(id, END, "", false, false, false);
    assert!(engine.handle_text_key(id, LEFT, "", false, true, false));
    assert!(engine.handle_text_key(id, LEFT, "", false, true, false));
    type_text(&mut engine, id, "p");
    assert_eq!(value_of(&mut engine, id, "q"), "help");
}

#[test]
fn a_paste_fires_beforeinput_with_insert_from_paste() {
    let (mut engine, id) = page();
    engine.clipboard().write_text("XY");
    focus(&mut engine, id, "e");
    assert!(chord(&mut engine, id, "v", false));
    assert_eq!(
        js(&mut engine, id, "log.join(' ')"),
        "beforeinput:insertFromPaste:XY input:insertFromPaste:XY"
    );
    assert_eq!(value_of(&mut engine, id, "e"), "XY");
}

#[test]
fn exec_command_copy_and_navigator_clipboard_use_the_engines_clipboard() {
    let (mut engine, id) = page();
    let platform = Rc::new(MemoryClipboard::new());
    engine.set_clipboard(platform.clone());
    // A trusted (chrome) view may read the clipboard from script.
    engine.set_script_clipboard_read_allowed(id, true);
    focus(&mut engine, id, "q");
    assert_eq!(
        js(
            &mut engine,
            id,
            "$('q').setSelectionRange(0, 4); document.execCommand('copy')"
        ),
        "true"
    );
    assert_eq!(platform.read_text().as_deref(), Some("hell"));
    js(
        &mut engine,
        id,
        "window.got = ''; navigator.clipboard.writeText('x').then(function () { \
         return navigator.clipboard.readText(); }).then(function (t) { got = t; }); 0",
    );
    // The promise reactions are microtasks; execute_script does not drain them.
    engine.views[&id]
        .bindings
        .as_ref()
        .unwrap()
        .run_jobs()
        .unwrap();
    assert_eq!(js(&mut engine, id, "got"), "x");
    assert_eq!(platform.read_text().as_deref(), Some("x"));
    // The user's paste reads the same clipboard.
    engine.handle_text_key(id, END, "", false, false, false);
    assert!(chord(&mut engine, id, "v", false));
    assert_eq!(value_of(&mut engine, id, "q"), "hellox");
}

#[test]
fn a_web_page_cannot_read_the_clipboard_from_script() {
    let (mut engine, id) = page();
    engine.clipboard().write_text("copied in another tab");
    js(
        &mut engine,
        id,
        "window.got = ''; navigator.clipboard.readText().then(function (t) { got = 'read:' + t; }, \
         function (e) { got = e.name; }); 0",
    );
    engine.views[&id]
        .bindings
        .as_ref()
        .unwrap()
        .run_jobs()
        .unwrap();
    assert_eq!(js(&mut engine, id, "got"), "NotAllowedError");
    focus(&mut engine, id, "e");
    assert_eq!(
        js(&mut engine, id, "document.execCommand('paste')"),
        "false"
    );
    assert_eq!(value_of(&mut engine, id, "e"), "");
    // The opt-in holds across a new document in the view.
    engine.set_script_clipboard_read_allowed(id, true);
    engine.load_html(id, PAGE).expect("reload");
    js(
        &mut engine,
        id,
        "window.got = ''; navigator.clipboard.readText().then(function (t) { got = t; }); 0",
    );
    engine.views[&id]
        .bindings
        .as_ref()
        .unwrap()
        .run_jobs()
        .unwrap();
    assert_eq!(js(&mut engine, id, "got"), "copied in another tab");
    // The user's own paste always reads it.
    engine.set_script_clipboard_read_allowed(id, false);
    js(
        &mut engine,
        id,
        "var $ = function (i) { return document.getElementById(i); }; 0",
    );
    focus(&mut engine, id, "e");
    assert!(chord(&mut engine, id, "v", false));
    assert_eq!(value_of(&mut engine, id, "e"), "copied in another tab");
}
