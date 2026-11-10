//! CH3.A4c: the Inspector page's console costs (`ui/inspector.html`), on a
//! headless engine view as the RustKit Inspector panel runs it.
//!
//! LAT7's read of a hand-test log: the Inspector view forced a layout in
//! every task that brought it console lines (100 in five minutes), each
//! with a child_list write and 4-108 text writes. The page rebuilt every
//! row with `innerHTML` on each line and then read `scrollHeight` to
//! scroll to the bottom.

use rustkit_engine::{Engine, EngineConfig, EngineViewId};
use rustkit_viewhost::Bounds;

fn inspector() -> (Engine, EngineViewId) {
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds::new(0, 0, 800, 300))
        .expect("view");
    engine.load_html(view, crate::INSPECTOR_HTML).expect("inspector page");
    (engine, view)
}

fn eval(engine: &mut Engine, view: EngineViewId, js: &str) -> String {
    let value = engine.execute_script(view, js).expect("script");
    value
        .strip_prefix("String(\"")
        .and_then(|v| v.strip_suffix("\")"))
        .unwrap_or(&value)
        .to_string()
}

fn forced(engine: &Engine, view: EngineViewId) -> u64 {
    engine.relayout_stats(view).expect("stats").forced_layouts
}

/// Let the page's frames and timers run, as the panel's live loop does.
fn frames(engine: &mut Engine, view: EngineViewId, n: usize) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    for _ in 0..n {
        std::thread::sleep(std::time::Duration::from_millis(17));
        rt.block_on(engine.pump_live(view, 17));
    }
}

/// One console line per task, as the panel is sent them while a page logs.
fn log_lines(engine: &mut Engine, view: EngineViewId, from: usize, n: usize) {
    for i in from..from + n {
        eval(
            engine,
            view,
            &format!(
                "window.hiwaveInspector.addConsoleEntries([{{id: {i}, level: 'log', \
                 message: 'line {i}', doc: 1, timestamp: 1}}]); 0"
            ),
        );
    }
}

fn rows(engine: &mut Engine, view: EngineViewId) -> usize {
    eval(engine, view, "String(document.querySelectorAll('#console-output .console-entry').length)")
        .parse()
        .unwrap_or(usize::MAX)
}

#[test]
fn fifty_console_lines_force_at_most_one_layout_on_the_console_tab() {
    let (mut engine, view) = inspector();
    frames(&mut engine, view, 2);
    eval(&mut engine, view, "document.querySelector('[data-tab=console]').click(); 0");
    frames(&mut engine, view, 2);
    let before = forced(&engine, view);
    log_lines(&mut engine, view, 0, 50);
    frames(&mut engine, view, 4);
    assert_eq!(rows(&mut engine, view), 50, "every line is shown");
    let spent = forced(&engine, view) - before;
    assert!(spent <= 1, "50 console lines forced {spent} layouts");
    // Still scrolled to the newest line.
    let pinned = eval(
        &mut engine,
        view,
        "var o = document.getElementById('console-output'); \
         String(o.scrollHeight - o.clientHeight - o.scrollTop <= 1)",
    );
    assert_eq!(pinned, "true", "the console follows the newest line");
}

#[test]
fn console_lines_on_the_elements_tab_force_no_layout() {
    let (mut engine, view) = inspector();
    frames(&mut engine, view, 2);
    let before = forced(&engine, view);
    log_lines(&mut engine, view, 0, 50);
    frames(&mut engine, view, 4);
    assert_eq!(forced(&engine, view) - before, 0, "the console is not on screen");
    // Opening the console shows them, at the newest.
    eval(&mut engine, view, "document.querySelector('[data-tab=console]').click(); 0");
    frames(&mut engine, view, 4);
    assert_eq!(rows(&mut engine, view), 50);
}

#[test]
fn a_later_line_appends_one_row_and_keeps_the_rows_before_it() {
    let (mut engine, view) = inspector();
    eval(&mut engine, view, "document.querySelector('[data-tab=console]').click(); 0");
    log_lines(&mut engine, view, 0, 3);
    frames(&mut engine, view, 3);
    eval(&mut engine, view, "window.__first = document.querySelector('#console-output .console-entry'); 0");
    log_lines(&mut engine, view, 3, 1);
    frames(&mut engine, view, 3);
    assert_eq!(rows(&mut engine, view), 4);
    let kept = eval(
        &mut engine,
        view,
        "String(window.__first === document.querySelector('#console-output .console-entry'))",
    );
    assert_eq!(kept, "true", "the first row was not rebuilt");
    // Errors and warnings are still counted.
    eval(
        &mut engine,
        view,
        "window.hiwaveInspector.addConsoleEntries([{id: 9, level: 'error', message: 'e', doc: 1}, \
         {id: 10, level: 'warn', message: 'w', doc: 1}, {id: 11, level: 'error', message: 'e2', doc: 1}]); 0",
    );
    frames(&mut engine, view, 3);
    let counts = eval(
        &mut engine,
        view,
        "document.getElementById('error-count').textContent + ',' + document.getElementById('warn-count').textContent",
    );
    assert_eq!(counts, "2,1");
}

#[test]
fn a_new_document_marks_the_earlier_lines_stale_with_a_divider() {
    let (mut engine, view) = inspector();
    eval(&mut engine, view, "document.querySelector('[data-tab=console]').click(); 0");
    log_lines(&mut engine, view, 0, 2);
    frames(&mut engine, view, 3);
    eval(
        &mut engine,
        view,
        "window.hiwaveInspector.addConsoleEntries([{id: 5, level: 'log', message: 'next', doc: 2}]); 0",
    );
    frames(&mut engine, view, 3);
    let shape = eval(
        &mut engine,
        view,
        "String(document.querySelectorAll('#console-output .console-doc-divider').length + ',' + \
         document.querySelectorAll('#console-output .console-entry.stale-doc').length + ',' + \
         document.querySelectorAll('#console-output .console-entry').length)",
    );
    assert_eq!(shape, "2,2,3", "dividers for both documents, the first document's lines stale");
}

/// The messages of the rows shown, in order.
fn shown(engine: &mut Engine, view: EngineViewId) -> String {
    eval(
        engine,
        view,
        "Array.from(document.querySelectorAll('#console-output .console-entry .console-entry-content'))\
         .map(e => e.textContent).join('|')",
    )
}

/// `n` lines of `level`, messages `{prefix} {i}`, in one task.
fn batch_js(prefix: &str, level: &str, from: usize, n: usize) -> String {
    let entries: Vec<String> = (from..from + n)
        .map(|i| format!("{{id: {i}, level: '{level}', message: '{prefix} {i}', doc: 1, timestamp: 1}}"))
        .collect();
    format!("window.hiwaveInspector.addConsoleEntries([{}]);", entries.join(", "))
}

fn console_tab(engine: &mut Engine, view: EngineViewId) {
    eval(engine, view, "document.querySelector('[data-tab=console]').click(); 0");
}

#[test]
fn a_clear_then_as_many_new_lines_in_one_frame_shows_only_the_new_lines() {
    let (mut engine, view) = inspector();
    console_tab(&mut engine, view);
    log_lines(&mut engine, view, 1000, 3);
    frames(&mut engine, view, 3);
    assert_eq!(shown(&mut engine, view), "line 1000|line 1001|line 1002");
    // Cleared, and more lines than were shown before the next frame.
    let js = format!("window.hiwaveInspector.clearConsole(); {} 0", batch_js("new", "log", 2000, 5));
    eval(&mut engine, view, &js);
    frames(&mut engine, view, 3);
    assert_eq!(
        shown(&mut engine, view),
        "new 2000|new 2001|new 2002|new 2003|new 2004",
        "the cleared rows are gone and every new line is shown"
    );
}

#[test]
fn a_clear_under_a_filter_then_new_lines_shows_only_the_new_lines() {
    let (mut engine, view) = inspector();
    console_tab(&mut engine, view);
    eval(&mut engine, view, "document.querySelector('.console-filter-btn[data-filter=error]').click(); 0");
    let js = format!("{} 0", batch_js("old", "error", 100, 3));
    eval(&mut engine, view, &js);
    frames(&mut engine, view, 3);
    assert_eq!(shown(&mut engine, view), "old 100|old 101|old 102");
    let js = format!(
        "window.hiwaveInspector.clearConsole(); {} {} 0",
        batch_js("new", "error", 200, 3),
        batch_js("plain", "log", 300, 2)
    );
    eval(&mut engine, view, &js);
    frames(&mut engine, view, 3);
    assert_eq!(shown(&mut engine, view), "new 200|new 201|new 202", "errors since the clear");
    // Back to every level: the new lines only.
    eval(&mut engine, view, "document.querySelector('.console-filter-btn[data-filter=all]').click(); 0");
    frames(&mut engine, view, 3);
    assert_eq!(
        shown(&mut engine, view),
        "new 200|new 201|new 202|plain 300|plain 301"
    );
}

fn scroll_state(engine: &mut Engine, view: EngineViewId) -> (i64, i64) {
    let s = eval(
        engine,
        view,
        "var o = document.getElementById('console-output'); \
         String(Math.round(o.scrollTop)) + ',' + String(Math.round(o.scrollHeight - o.clientHeight))",
    );
    let mut it = s.split(',').map(|v| v.parse::<i64>().unwrap_or(-1));
    (it.next().unwrap(), it.next().unwrap())
}

#[test]
fn a_user_scroll_up_while_lines_stream_is_kept_and_the_bottom_resumes_following() {
    let (mut engine, view) = inspector();
    console_tab(&mut engine, view);
    frames(&mut engine, view, 2);
    log_lines(&mut engine, view, 0, 30);
    frames(&mut engine, view, 3);
    let (top, max) = scroll_state(&mut engine, view);
    assert!(max > 200, "the console overflows ({max})");
    assert!(top >= max - 1, "following the newest line ({top} of {max})");
    // 50 lines, one a frame; with the eleventh the person scrolls to the
    // top in the same frame the console follows the line, just after it
    // (the wheel lands while lines stream).
    for i in 0..50 {
        log_lines(&mut engine, view, 100 + i, 1);
        if i == 10 {
            eval(
                &mut engine,
                view,
                "requestAnimationFrame(() => { document.getElementById('console-output').scrollTop = 0; }); 0",
            );
        }
        frames(&mut engine, view, 1);
    }
    frames(&mut engine, view, 3);
    let (top, max) = scroll_state(&mut engine, view);
    assert!(top < 50, "the person's scroll was kept: scrollTop {top} of {max}");
    // Back at the bottom: following resumes.
    eval(
        &mut engine,
        view,
        "var o = document.getElementById('console-output'); o.scrollTop = o.scrollHeight; 0",
    );
    frames(&mut engine, view, 3);
    for i in 0..10 {
        log_lines(&mut engine, view, 200 + i, 1);
        frames(&mut engine, view, 1);
    }
    frames(&mut engine, view, 3);
    let (top, max) = scroll_state(&mut engine, view);
    assert!(top >= max - 1, "following the newest line again ({top} of {max})");
}


#[test]
fn the_first_lines_show_in_the_task_that_brings_them() {
    // The chrome board (and a person) takes a frame before the page's next
    // animation frame runs: the lines must already be rows by then.
    let (mut engine, view) = inspector();
    frames(&mut engine, view, 2);
    let js = format!(
        "{} String(document.querySelectorAll('#console-output .console-entry').length) + ',' + \
         document.getElementById('error-count').textContent + ',' + \
         document.getElementById('console-empty').style.display",
        batch_js("first", "error", 0, 4)
    );
    let same_task = eval(&mut engine, view, &js);
    assert!(same_task.starts_with("4,4,"), "rows and counts in the same task: {same_task}");
    assert!(same_task.ends_with(",none"), "the empty state is hidden: {same_task}");
    // On the Console tab with no frame run, the rows are there too.
    console_tab(&mut engine, view);
    assert_eq!(rows(&mut engine, view), 4);
    // Lines while a frame is already due are coalesced into it.
    let js = format!(
        "{} {} String(document.querySelectorAll('#console-output .console-entry').length)",
        batch_js("more", "log", 10, 1),
        batch_js("more", "log", 11, 1)
    );
    let _ = eval(&mut engine, view, &js);
    frames(&mut engine, view, 3);
    assert_eq!(rows(&mut engine, view), 6);
}

// SEC1b: on the WebKit content path the console lines, DOM tree and picked
// element the Inspector shows are posted by the content page, which can
// set every field to any type. They must reach the Inspector page (which
// holds `window.ipc`) as data: no element and no script from them.

/// Arm the harmless flag; after the page took the data, click whatever
/// element it may have made from it, and read the flag and the count of
/// such elements.
fn arm(engine: &mut Engine, view: EngineViewId) {
    eval(engine, view, "window.__flag = 0; 0");
}

fn injected_and_flag(engine: &mut Engine, view: EngineViewId) -> String {
    eval(
        engine,
        view,
        "var e = document.getElementById('injected'); if (e) e.click(); \
         String(document.querySelectorAll('#injected').length) + ',' + String(window.__flag)",
    )
}

/// Markup that, if it lands in an attribute value or text unescaped, makes
/// `#injected`, whose click sets the flag.
const MARKUP: &str = "\"><span id=\"injected\" onclick=\"window.__flag=1\">x</span><i data-x=\"";

#[test]
fn a_console_line_with_a_non_numeric_doc_shows_no_markup() {
    let (mut engine, view) = inspector();
    console_tab(&mut engine, view);
    arm(&mut engine, view);
    let entry = serde_json::json!({
        "id": 1, "level": "log", "message": "m", "doc": MARKUP, "timestamp": 1
    });
    eval(&mut engine, view, &format!("window.hiwaveInspector.addConsoleEntry({entry}); 0"));
    frames(&mut engine, view, 3);
    assert_eq!(injected_and_flag(&mut engine, view), "0,0", "no element, no script from the doc");
    assert_eq!(shown(&mut engine, view), "m", "the line itself is shown");
}

#[test]
fn a_dom_tree_with_a_string_children_omitted_shows_no_markup() {
    let (mut engine, view) = inspector();
    arm(&mut engine, view);
    let tree = serde_json::json!({
        "type": "element", "tag": "html", "path": "html", "classes": [],
        "children": [{
            "type": "element", "tag": "div", "path": "html > div", "classes": [],
            "children": [], "childrenOmitted": MARKUP
        }]
    });
    eval(&mut engine, view, &format!("window.hiwaveInspector.setDomTree({tree}); 0"));
    frames(&mut engine, view, 2);
    assert_eq!(injected_and_flag(&mut engine, view), "0,0", "no element, no script from childrenOmitted");
    assert_eq!(
        eval(&mut engine, view, "String(document.querySelectorAll('#dom-tree .dom-node').length)"),
        "1",
        "the node itself is shown"
    );
}

/// A picked element's path reaches `selectElementByPath` exactly as the
/// page sent it, whatever it holds, and nothing in it runs.
#[test]
fn a_picked_path_reaches_select_element_by_path_intact() {
    let paths = [
        // A backslash before a quote, then code and a line comment.
        "div\\'); window.__flag = 1; } //",
        // Quotes, a backslash and line breaks.
        "div[title=\"a\\\\b\"] > p:nth-child(2)\n'x'\r\u{2028}",
    ];
    for path in paths {
        let (mut engine, view) = inspector();
        arm(&mut engine, view);
        eval(
            &mut engine,
            view,
            "window.__got = null; \
             window.hiwaveInspector.selectElementByPath = function (p) { window.__got = p; }; 0",
        );
        let script = crate::inspector_picked_script(path, "null");
        let _ = engine.execute_script(view, &script);
        assert_eq!(eval(&mut engine, view, "String(window.__flag)"), "0", "nothing in {path:?} ran");
        let literal = serde_json::to_string(path).unwrap();
        assert_eq!(
            eval(&mut engine, view, &format!("String(window.__got === {literal})")),
            "true",
            "the path arrives intact: {path:?}"
        );
    }
}

/// The colour preview takes only a colour: a value with more declarations
/// in it gets no preview (the value itself is still listed, as text).
#[test]
fn a_colour_preview_is_only_made_for_a_plain_colour() {
    let (mut engine, view) = inspector();
    let styles = serde_json::json!({
        "selector": "div",
        "computedStyles": {
            "color": "rgb(1, 2, 3)",
            "background-color": "rgb(0, 0, 0; position: fixed; inset: 0)",
            "border-color": "rgba(0,0,0,0.5)"
        }
    });
    eval(&mut engine, view, &format!("window.hiwaveInspector.setElementStyles({styles}); 0"));
    assert_eq!(
        eval(&mut engine, view, "String(document.querySelectorAll('#computed-styles .color-preview').length)"),
        "2",
        "previews for the two plain colours only"
    );
    assert_eq!(
        eval(
            &mut engine,
            view,
            "String(Array.from(document.querySelectorAll('#computed-styles .style-property-value'))\
             .some(v => v.textContent.includes('position: fixed')))",
        ),
        "true",
        "the forged value is still listed as text"
    );
}

/// A console message `0` shows as `0`, not as an empty line.
#[test]
fn a_zero_message_shows_as_zero() {
    let (mut engine, view) = inspector();
    console_tab(&mut engine, view);
    eval(
        &mut engine,
        view,
        "window.hiwaveInspector.addConsoleEntry({id: 0, level: 'log', message: 0, doc: 1, timestamp: 1}); 0",
    );
    frames(&mut engine, view, 3);
    assert_eq!(shown(&mut engine, view), "0");
}

/// autofill.js (injected into content pages) builds no markup from what it
/// shows: #1034 replaced its string escaping (`escapeHtml` into `innerHTML`)
/// with `textContent`, so there is no escaper left to test. This pins the
/// source instead: no HTML-parsing sink, and `textContent` for the text.
#[test]
fn the_autofill_helper_builds_no_markup_from_entries() {
    let src = crate::AUTOFILL_HELPER;
    for sink in ["innerHTML", "insertAdjacentHTML", "outerHTML", "document.write"] {
        assert!(!src.contains(sink), "autofill.js uses {sink}");
    }
    assert!(src.contains("textContent"), "autofill.js sets its text with textContent");
}
