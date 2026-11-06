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
