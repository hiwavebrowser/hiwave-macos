//! The Inspector panel's native path for RustKit content: the panel
//! (`ui/inspector.html`) asks, this module asks the page's developer tools
//! (`rustkit_engine::devtools`, through `PageCmd::Devtools`) for the
//! current tab's view and hands the answers back through the panel's
//! `window.hiwaveInspector` API.
//!
//! WebKit content keeps the injected-script inspector (`ui/inspector_inject.js`);
//! RustKit content gets no script injected: page script cannot see or
//! interfere with the Inspector. Every query is on demand and bounded and
//! stays in the app: no network, no remote debugging protocol.
//!
//! The page answers on its own thread when the content thread runs it (the
//! shipped default), so nothing here waits: a question is sent, and the
//! answer is acted on by [`Devtools::pump`], which the event loop calls on
//! every live wake. On the same-thread path the answer is ready when the
//! question returns and the next pump takes it.

use crate::webview_rustkit::RustKitView;
use rustkit_engine::{devtools_node_of, devtools_path, DevtoolsAnswer, DevtoolsQuery};
use serde::Serialize;
use tokio::sync::oneshot;
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

/// What an answer is for.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Want {
    /// The whole tree, expanded down to the selection.
    Tree,
    /// The children of a node the tree left out.
    Subtree,
    /// The selected element's details.
    Element,
    /// Picking: the element under the pointer moved.
    Hover,
    /// Picking: the element under the released pointer.
    Pick,
    /// Console lines since the last ones shown.
    Console,
    /// An outline or the recording switched; nothing to show.
    Ack,
}

enum Asked {
    Page(Want, oneshot::Receiver<DevtoolsAnswer>),
    Eval(oneshot::Receiver<Result<String, String>>),
}

/// The Inspector's state for the current tab.
#[derive(Default)]
pub struct Devtools {
    open: bool,
    /// Select-element mode: the page's pointer input picks instead.
    picking: bool,
    hovered: Option<usize>,
    /// A pointer move that arrived while the last one was being answered.
    /// Only the newest is asked about.
    hover_next: Option<Option<(f32, f32)>>,
    selected: Option<usize>,
    /// The next Inspector record the console has not shown.
    console_seq: u64,
    asked: Vec<Asked>,
}

impl Devtools {
    pub fn picking(&self) -> bool {
        self.picking
    }

    fn ask(&mut self, view: &RustKitView, want: Want, query: DevtoolsQuery) {
        self.asked.push(Asked::Page(want, view.devtools_ask(query)));
    }

    fn waiting_for(&self, want: Want) -> bool {
        self.asked.iter().any(|a| matches!(a, Asked::Page(w, _) if *w == want))
    }

    /// The panel opened: record the page's console from now on, and send
    /// the DOM tree and what the console holds.
    pub fn open(&mut self, view: &RustKitView, _panel: &WebView) {
        self.open = true;
        self.console_seq = 0;
        self.ask(view, Want::Ack, DevtoolsQuery::Attach(true));
        // The element first: its breadcrumb is what the tree keeps open.
        if let Some(n) = self.selected {
            self.ask(view, Want::Element, DevtoolsQuery::Element(n));
        }
        self.send_tree(view);
        self.ask(view, Want::Console, DevtoolsQuery::Console { since: 0 });
    }

    /// The panel closed: no outline, no picking, no recording.
    pub fn close(&mut self, view: &RustKitView, panel: &WebView) {
        self.stop_picker(view, panel);
        self.open = false;
        self.asked.clear();
        self.ask(view, Want::Ack, DevtoolsQuery::Highlight(None));
        self.ask(view, Want::Ack, DevtoolsQuery::Attach(false));
        run(panel, "window.hiwaveInspector && window.hiwaveInspector.clearConsole()");
    }

    /// The DOM tree again, expanded down to the selection.
    pub fn send_tree(&mut self, view: &RustKitView) {
        let reveal = self.selected;
        self.ask(view, Want::Tree, DevtoolsQuery::DomTree { root: None, reveal });
    }

    /// The children of the node `path` names, which the tree left out.
    pub fn expand(&mut self, view: &RustKitView, path: &str) {
        if let Some(n) = devtools_node_of(path) {
            self.ask(view, Want::Subtree, DevtoolsQuery::DomTree { root: Some(n), reveal: None });
        }
    }

    /// The panel selected `path` (a tree row, a breadcrumb).
    pub fn select(&mut self, view: &RustKitView, path: &str) {
        if let Some(n) = devtools_node_of(path) {
            self.selected = Some(n);
            self.ask(view, Want::Element, DevtoolsQuery::Element(n));
        }
    }

    /// Outline the node `path` names on the page (a tree row is hovered),
    /// or nothing.
    pub fn highlight(&mut self, view: &RustKitView, path: Option<&str>) {
        self.ask(view, Want::Ack, DevtoolsQuery::Highlight(path.and_then(devtools_node_of)));
    }

    pub fn start_picker(&mut self, panel: &WebView) {
        self.picking = true;
        run(panel, "window.hiwaveInspector && window.hiwaveInspector.setPickerActive(true)");
    }

    pub fn stop_picker(&mut self, view: &RustKitView, panel: &WebView) {
        if self.picking {
            self.picking = false;
            self.hovered = None;
            self.hover_next = None;
            self.ask(view, Want::Ack, DevtoolsQuery::Highlight(None));
        }
        run(panel, "window.hiwaveInspector && window.hiwaveInspector.setPickerActive(false)");
    }

    /// Picking, the pointer moved to (`x`, `y`) in the view (or left it):
    /// outline the element under it, and mark its row. One question is out
    /// at a time; a move during it replaces the one waiting.
    pub fn pick_hover(&mut self, view: &RustKitView, panel: &WebView, at: Option<(f32, f32)>) {
        if self.waiting_for(Want::Hover) {
            self.hover_next = Some(at);
            return;
        }
        match at {
            Some((x, y)) => self.ask(view, Want::Hover, DevtoolsQuery::NodeAt { x, y }),
            None => self.hovered_now(view, panel, None),
        }
    }

    /// Picking, the pointer was released at (`x`, `y`): select the element
    /// there and leave select-element mode.
    pub fn pick_at(&mut self, view: &RustKitView, panel: &WebView, x: f32, y: f32) {
        self.stop_picker(view, panel);
        self.ask(view, Want::Pick, DevtoolsQuery::NodeAt { x, y });
    }

    /// The console's input line: run `code` in the page and show the result.
    pub fn eval(&mut self, view: &RustKitView, code: &str) {
        self.asked.push(Asked::Eval(view.devtools_eval(code)));
    }

    /// Act on the answers that have arrived, then ask for new console
    /// lines. Called on every live wake of the content view.
    pub fn pump(&mut self, view: &RustKitView, panel: &WebView) {
        if !self.open {
            self.asked.clear();
            return;
        }
        let mut arrived = Vec::new();
        self.asked.retain_mut(|asked| match asked {
            Asked::Page(want, rx) => match rx.try_recv() {
                Ok(answer) => {
                    arrived.push(Arrived::Page(*want, answer));
                    false
                }
                Err(oneshot::error::TryRecvError::Empty) => true,
                // A closed view, or a command the same-thread path dropped
                // while a load held the page.
                Err(oneshot::error::TryRecvError::Closed) => {
                    arrived.push(Arrived::Page(*want, DevtoolsAnswer::Unavailable));
                    false
                }
            },
            Asked::Eval(rx) => match rx.try_recv() {
                Ok(result) => {
                    arrived.push(Arrived::Eval(result));
                    false
                }
                Err(oneshot::error::TryRecvError::Empty) => true,
                Err(oneshot::error::TryRecvError::Closed) => {
                    arrived.push(Arrived::Eval(Err("the page is loading".into())));
                    false
                }
            },
        });
        for arrival in arrived {
            match arrival {
                Arrived::Page(want, answer) => self.answered(view, panel, want, answer),
                Arrived::Eval(result) => {
                    let (text, is_error) = match result {
                        Ok(value) => (value, false),
                        Err(e) => (e, true),
                    };
                    let json = serde_json::to_string(&text).unwrap_or_else(|_| "\"\"".into());
                    run(
                        panel,
                        &format!("window.hiwaveInspector && window.hiwaveInspector.addConsoleResult({json}, {is_error})"),
                    );
                }
            }
        }
        if self.open && !self.waiting_for(Want::Console) {
            let since = self.console_seq;
            self.ask(view, Want::Console, DevtoolsQuery::Console { since });
        }
    }

    fn hovered_now(&mut self, view: &RustKitView, panel: &WebView, node: Option<usize>) {
        if node == self.hovered {
            return;
        }
        self.hovered = node;
        self.ask(view, Want::Ack, DevtoolsQuery::Highlight(node));
        run(panel, &panel_call("highlightElement", &node.map(devtools_path)));
    }

    fn answered(&mut self, view: &RustKitView, panel: &WebView, want: Want, answer: DevtoolsAnswer) {
        match (want, answer) {
            (Want::Tree, DevtoolsAnswer::Tree(Some(tree))) => {
                run(panel, &panel_call("setDomTree", &tree));
                if let Some(n) = self.selected {
                    run(panel, &panel_call("markSelected", &devtools_path(n)));
                }
            }
            (Want::Tree, _) => run(panel, "window.hiwaveInspector && window.hiwaveInspector.clear()"),
            (Want::Subtree, DevtoolsAnswer::Tree(Some(subtree))) => {
                run(panel, &panel_call("setSubtree", &subtree));
            }
            (Want::Element, DevtoolsAnswer::Element(Some(element))) => {
                run(panel, &panel_call("setElementStyles", &element));
            }
            (Want::Element, _) => {
                run(panel, "window.hiwaveInspector && window.hiwaveInspector.setElementStyles(null)");
            }
            (Want::Hover, DevtoolsAnswer::Node(node)) => {
                if self.picking {
                    self.hovered_now(view, panel, node);
                }
                if let Some(next) = self.hover_next.take() {
                    if self.picking {
                        self.pick_hover(view, panel, next);
                    }
                }
            }
            (Want::Pick, DevtoolsAnswer::Node(Some(n))) => {
                self.selected = Some(n);
                self.ask(view, Want::Element, DevtoolsQuery::Element(n));
                self.send_tree(view);
            }
            (Want::Console, DevtoolsAnswer::Console(console)) => {
                if console.next_seq == self.console_seq {
                    return;
                }
                self.console_seq = console.next_seq;
                if !console.entries.is_empty() {
                    run(panel, &panel_call("addConsoleEntries", &console.entries));
                }
                // Node ids do not survive a navigation: a new page loses
                // its selection and gets a new tree.
                if console.page_changed {
                    self.selected = None;
                    self.send_tree(view);
                    run(panel, "window.hiwaveInspector && window.hiwaveInspector.setElementStyles(null)");
                }
            }
            _ => {}
        }
    }
}

enum Arrived {
    Page(Want, DevtoolsAnswer),
    Eval(Result<String, String>),
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
