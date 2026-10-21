//! The Inspector panel's native path for RustKit content: the panel
//! (`ui/inspector.html`) asks, this module queries the engine's developer
//! tools (`rustkit_engine::devtools`) for the current tab's view and hands
//! the answers back through the panel's `window.hiwaveInspector` API.
//!
//! WebKit content keeps the injected-script inspector (`ui/inspector_inject.js`);
//! RustKit content gets no script injected: page script cannot see or
//! interfere with the Inspector. Every query is on demand and bounded, runs
//! between the page's script turns on the window thread, and stays in the
//! app: no network, no remote debugging protocol.

use crate::webview_rustkit::RustKitView;
use rustkit_engine::{devtools_node_of, devtools_path};
use serde::Serialize;
use wry::WebView;

/// A call of the panel's API with `arg` as JSON. A JSON text is a JS
/// expression, so a string the page chose (a class name, a console line)
/// stays data.
pub fn panel_call(method: &str, arg: &impl Serialize) -> String {
    let json = serde_json::to_string(arg).unwrap_or_else(|_| "null".into());
    format!("window.hiwaveInspector && window.hiwaveInspector.{method}({json})")
}

fn run(panel: &WebView, script: &str) {
    let _ = panel.evaluate_script(script);
}

/// The Inspector's state for the current tab.
#[derive(Default)]
pub struct Devtools {
    open: bool,
    /// Select-element mode: the page's pointer input picks instead.
    picking: bool,
    hovered: Option<usize>,
    selected: Option<usize>,
    /// The next Inspector record the console has not shown.
    console_seq: u64,
}

impl Devtools {
    pub fn picking(&self) -> bool {
        self.picking
    }

    /// The panel opened: record the page's console from now on, and send
    /// the DOM tree and what the console holds.
    pub fn open(&mut self, view: &RustKitView, panel: &WebView) {
        self.open = true;
        self.console_seq = 0;
        view.devtools_attach(true);
        // The element first: its breadcrumb is what the tree keeps open.
        if let Some(n) = self.selected {
            self.send_element(view, panel, n);
        }
        self.send_tree(view, panel);
        self.poll_console(view, panel);
    }

    /// The panel closed: no outline, no picking, no recording.
    pub fn close(&mut self, view: &RustKitView, panel: &WebView) {
        self.stop_picker(view, panel);
        self.open = false;
        view.devtools_highlight(None);
        view.devtools_attach(false);
        run(panel, "window.hiwaveInspector && window.hiwaveInspector.clearConsole()");
    }

    /// The DOM tree again, expanded down to the selection.
    pub fn send_tree(&self, view: &RustKitView, panel: &WebView) {
        match view.devtools_dom_tree(None, self.selected) {
            Some(tree) => {
                run(panel, &panel_call("setDomTree", &tree));
                if let Some(n) = self.selected {
                    run(panel, &panel_call("markSelected", &devtools_path(n)));
                }
            }
            None => run(panel, "window.hiwaveInspector && window.hiwaveInspector.clear()"),
        }
    }

    /// The children of the node `path` names, which the tree left out.
    pub fn expand(&self, view: &RustKitView, panel: &WebView, path: &str) {
        if let Some(subtree) = devtools_node_of(path).and_then(|n| view.devtools_dom_tree(Some(n), None)) {
            run(panel, &panel_call("setSubtree", &subtree));
        }
    }

    /// The panel selected `path` (a tree row, a breadcrumb).
    pub fn select(&mut self, view: &RustKitView, panel: &WebView, path: &str) {
        if let Some(n) = devtools_node_of(path) {
            self.selected = Some(n);
            self.send_element(view, panel, n);
        }
    }

    fn send_element(&self, view: &RustKitView, panel: &WebView, node: usize) {
        match view.devtools_element(node) {
            Some(element) => run(panel, &panel_call("setElementStyles", &element)),
            None => run(panel, "window.hiwaveInspector && window.hiwaveInspector.setElementStyles(null)"),
        }
    }

    /// Outline the node `path` names on the page (a tree row is hovered),
    /// or nothing.
    pub fn highlight(&self, view: &RustKitView, path: Option<&str>) {
        view.devtools_highlight(path.and_then(devtools_node_of));
    }

    pub fn start_picker(&mut self, panel: &WebView) {
        self.picking = true;
        run(panel, "window.hiwaveInspector && window.hiwaveInspector.setPickerActive(true)");
    }

    pub fn stop_picker(&mut self, view: &RustKitView, panel: &WebView) {
        if self.picking {
            self.picking = false;
            self.hovered = None;
            view.devtools_highlight(None);
        }
        run(panel, "window.hiwaveInspector && window.hiwaveInspector.setPickerActive(false)");
    }

    /// Picking, the pointer moved to (`x`, `y`) in the view: outline the
    /// element under it, and mark its row.
    pub fn pick_hover(&mut self, view: &RustKitView, panel: &WebView, at: Option<(f32, f32)>) {
        let node = at.and_then(|(x, y)| view.devtools_node_at(x, y));
        if node != self.hovered {
            self.hovered = node;
            view.devtools_highlight(node);
            run(panel, &panel_call("highlightElement", &node.map(devtools_path)));
        }
    }

    /// Picking, the pointer was released at (`x`, `y`): select the element
    /// there and leave select-element mode.
    pub fn pick_at(&mut self, view: &RustKitView, panel: &WebView, x: f32, y: f32) {
        let node = view.devtools_node_at(x, y);
        self.stop_picker(view, panel);
        if let Some(n) = node {
            self.selected = Some(n);
            self.send_element(view, panel, n);
            self.send_tree(view, panel);
        }
    }

    /// New console lines since the last poll; a page that changed gets a new
    /// tree (and loses its selection: node ids do not survive a navigation).
    pub fn poll_console(&mut self, view: &RustKitView, panel: &WebView) {
        if !self.open {
            return;
        }
        let Some(console) = view.devtools_console(self.console_seq) else { return };
        if console.next_seq == self.console_seq {
            return;
        }
        self.console_seq = console.next_seq;
        if !console.entries.is_empty() {
            run(panel, &panel_call("addConsoleEntries", &console.entries));
        }
        if console.page_changed {
            self.selected = None;
            self.send_tree(view, panel);
            run(panel, "window.hiwaveInspector && window.hiwaveInspector.setElementStyles(null)");
        }
    }

    /// The console's input line: run `code` in the page and show the result.
    pub fn eval(&self, view: &RustKitView, panel: &WebView, code: &str) {
        let (text, is_error) = match view.devtools_eval(code) {
            Ok(value) => (value, false),
            Err(e) => (e, true),
        };
        let json = serde_json::to_string(&text).unwrap_or_else(|_| "\"\"".into());
        run(panel, &format!("window.hiwaveInspector && window.hiwaveInspector.addConsoleResult({json}, {is_error})"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_call_passes_page_text_as_json_data() {
        let call = panel_call("highlightElement", &Some("rk:7"));
        assert_eq!(call, r#"window.hiwaveInspector && window.hiwaveInspector.highlightElement("rk:7")"#);
        let hostile = panel_call("addConsoleEntries", &["\"); alert(1); (\"", "</script>"]);
        assert!(hostile.ends_with(r#"addConsoleEntries(["\"); alert(1); (\"","</script>"])"#), "{hostile}");
        assert!(panel_call("highlightElement", &None::<String>).ends_with("highlightElement(null)"));
    }
}
