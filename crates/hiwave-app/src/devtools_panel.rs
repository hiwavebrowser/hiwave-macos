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
//! question returns and the next pump takes it. Console reads are not page
//! questions: the devtools thread answers them from the view's Inspector
//! (`rustkit_engine::devtools_ask_console`, T6a).

use std::time::{Duration, Instant};

use crate::webview_rustkit::RustKitView;
use rustkit_engine::{devtools_node_of, devtools_path, DevtoolsAnswer, DevtoolsConsole, DevtoolsQuery};
use serde::Serialize;
use tokio::sync::oneshot;
use wry::WebView;

/// How long the console's input line waits for a busy page before it is
/// given up and cancelled (T6c).
const EVAL_DEADLINE: Duration = Duration::from_secs(5);

/// How long a live snapshot (tree, element, node under the pointer,
/// outline) waits before the panel says the page is busy (T6c).
const LIVE_DEADLINE: Duration = Duration::from_secs(2);

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

/// Whether the open panel asks for new console lines now: only when the
/// view has recorded past what the panel has shown, and no question is
/// out. There is no clock (T6b): the devtools thread wakes the window when
/// a watched view records, so an idle page and an open panel cost nothing.
/// (#785 R1: asking again on every answer, while every answer woke the
/// window, kept the two waking each other without pause.)
fn console_due(watermark: Option<u64>, shown_to: u64, waiting: bool) -> bool {
    !waiting && watermark.is_some_and(|w| w > shown_to)
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
    /// The outline on the page moved; nothing to show.
    Highlight,
    /// The recording switched; nothing to show.
    Ack,
}

impl Want {
    /// A snapshot only the page can answer: it gets a deadline, and one of
    /// each kind is out at a time (T6c). Console reads are answered by the
    /// devtools thread; the recording switch must reach the page.
    fn live(self) -> bool {
        !matches!(self, Want::Console | Want::Ack)
    }
}

/// Which live questions replace each other: the newest of a kind, except
/// that expanding two nodes asks two different questions (#809 review 1:
/// the second expand replaced the first, whose "more" row then did nothing).
type Slot = (Want, Option<usize>);

fn slot(want: Want, query: &DevtoolsQuery) -> Slot {
    match (want, query) {
        (Want::Subtree, DevtoolsQuery::DomTree { root, .. }) => (want, *root),
        _ => (want, None),
    }
}

/// A question out to the page.
struct Question {
    want: Want,
    /// Kept to ask again once a late answer has come and gone.
    query: DevtoolsQuery,
    rx: oneshot::Receiver<DevtoolsAnswer>,
    asked_at: Instant,
    /// Past [`LIVE_DEADLINE`]: its answer, when it comes, is stale and
    /// discarded, and the newest question of its kind is asked instead.
    late: bool,
}

enum Asked {
    Page(Question),
    /// The console's input line, and when it is given up.
    Eval(oneshot::Receiver<Result<String, String>>, Instant),
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
    /// "page crashed" is on the console (T6 R1 condition 2).
    crash_shown: bool,
    /// The held console count on the panel's "K console lines held by a
    /// running script" line (0: no line).
    held_shown: usize,
    asked: Vec<Asked>,
    /// The newest live question of a kind asked while one of that kind
    /// was out, asked when it is answered (T6c: no repeats queue up).
    again: Vec<(Want, DevtoolsQuery)>,
    /// "page busy" is on the panel.
    busy_shown: Option<u64>,
}

impl Devtools {
    pub fn picking(&self) -> bool {
        self.picking
    }

    fn ask(&mut self, view: &RustKitView, want: Want, query: DevtoolsQuery) {
        self.ask_with(want, query, &mut |q| view.devtools_ask(q));
    }

    fn ask_with(&mut self, want: Want, query: DevtoolsQuery, send: &mut SendQuestion<'_>) {
        let at = slot(want, &query);
        if want.live() && self.waiting_in(at) {
            self.again.retain(|(w, q)| slot(*w, q) != at);
            self.again.push((want, query));
            return;
        }
        let rx = send(query.clone());
        self.asked.push(Asked::Page(Question { want, query, rx, asked_at: Instant::now(), late: false }));
    }

    fn waiting_for(&self, want: Want) -> bool {
        self.asked.iter().any(|a| matches!(a, Asked::Page(q) if q.want == want))
    }

    fn waiting_in(&self, at: Slot) -> bool {
        self.asked.iter().any(|a| matches!(a, Asked::Page(q) if slot(q.want, &q.query) == at))
    }

    /// The page's answers that have arrived, in the order asked. A live
    /// question past its deadline is marked late. A late answer is dropped
    /// when a newer question of its slot was wanted meanwhile (the
    /// pointer's newest position for a hover), and that one is asked;
    /// otherwise it is shown, since a late answer beats none (#809 review
    /// 2: a page always a little slower than the deadline was re-asked the
    /// same question forever and never showed a tree). Returns how long the
    /// oldest late question has waited, if one is out: the page is busy.
    ///
    /// Newer questions wait in `again` behind a late one until it answers,
    /// so on a page that never answers they wait for good; the busy line
    /// says why.
    fn take_page_answers(&mut self, now: Instant, send: &mut SendQuestion<'_>) -> (Vec<(Want, DevtoolsAnswer)>, Option<Duration>) {
        let mut arrived = Vec::new();
        let mut answered = Vec::new();
        let mut late = Vec::new();
        self.asked.retain_mut(|asked| {
            let Asked::Page(q) = asked else { return true };
            let answer = match q.rx.try_recv() {
                Ok(answer) => answer,
                Err(oneshot::error::TryRecvError::Empty) => {
                    if q.want.live() && now.saturating_duration_since(q.asked_at) >= LIVE_DEADLINE {
                        q.late = true;
                    }
                    return true;
                }
                // A closed view, or a command the same-thread path dropped
                // while a load held the page.
                Err(oneshot::error::TryRecvError::Closed) => DevtoolsAnswer::Unavailable,
            };
            if q.late {
                late.push((q.want, slot(q.want, &q.query), answer));
            } else {
                answered.push(slot(q.want, &q.query));
                arrived.push((q.want, answer));
            }
            false
        });
        for (want, at, answer) in late {
            let newest = match want {
                Want::Hover => match self.hover_next.take() {
                    Some(Some((x, y))) => Some(DevtoolsQuery::NodeAt { x, y }),
                    // The pointer left the view meanwhile: whatever was
                    // under it, the outline goes (#809 review 3).
                    Some(None) => {
                        arrived.push((want, DevtoolsAnswer::Node(None)));
                        continue;
                    }
                    None => None,
                },
                _ => self.take_again(at),
            };
            match newest {
                Some(query) => self.ask_with(want, query, send),
                None => arrived.push((want, answer)),
            }
        }
        // Answered in time: what was wanted meanwhile is asked now.
        for at in answered {
            if !self.waiting_in(at) {
                if let Some(query) = self.take_again(at) {
                    self.ask_with(at.0, query, send);
                }
            }
        }
        let busy = self
            .asked
            .iter()
            .filter_map(|a| match a {
                Asked::Page(q) if q.late => Some(now.saturating_duration_since(q.asked_at)),
                _ => None,
            })
            .max();
        (arrived, busy)
    }

    fn take_again(&mut self, at: Slot) -> Option<DevtoolsQuery> {
        let i = self.again.iter().position(|(w, q)| slot(*w, q) == at)?;
        Some(self.again.remove(i).1)
    }

    /// A hover had no answer (a closed view, or a load held the page).
    /// Nothing more is asked; true when the pointer left the view meanwhile,
    /// so the outline goes all the same (#809 re-review).
    fn hover_unavailable(&mut self) -> bool {
        self.hover_next.take() == Some(None) && self.picking
    }

    /// The panel closed: nothing out is shown, nothing waiting is asked.
    fn forget_questions(&mut self) {
        self.asked.clear();
        self.again.clear();
        self.busy_shown = None;
    }

    /// A new page: node ids do not survive a navigation, so the selection
    /// and outline go, and a live answer still out about the old page's
    /// nodes, or one waiting to be asked, is not shown on the new one.
    fn forget_page(&mut self) {
        self.selected = None;
        self.hovered = None;
        self.hover_next = None;
        self.asked.retain(|a| match a {
            Asked::Eval(..) => true,
            Asked::Page(q) => !q.want.live(),
        });
        self.again.clear();
    }

    /// The panel opened: record the page's console from now on, and send
    /// the DOM tree and what the console holds.
    pub fn open(&mut self, view: &RustKitView, _panel: &WebView) {
        self.open = true;
        self.console_seq = 0;
        self.crash_shown = false;
        self.held_shown = 0;
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
        self.close_with(&mut |q| view.devtools_ask(q), &mut |js| run(panel, js));
    }

    fn close_with(&mut self, send: &mut SendQuestion<'_>, show: &mut RunPanel<'_>) {
        self.stop_picker_with(send, show);
        self.open = false;
        self.forget_questions();
        show("window.hiwaveInspector && window.hiwaveInspector.setPageBusy(null)");
        self.ask_with(Want::Highlight, DevtoolsQuery::Highlight(None), send);
        self.ask_with(Want::Ack, DevtoolsQuery::Attach(false), send);
        self.clear_console_with(show);
    }

    /// The page cleared its console (WebKit content's `console.clear()`).
    pub fn clear_console(&mut self, panel: &WebView) {
        self.clear_console_with(&mut |js| run(panel, js));
    }

    /// The panel's `clearConsole()` hides the held line too, so the next
    /// pump shows it again while lines are still held (#809 follow-up).
    fn clear_console_with(&mut self, show: &mut RunPanel<'_>) {
        show("window.hiwaveInspector && window.hiwaveInspector.clearConsole()");
        self.held_shown = 0;
    }

    /// The "K console lines held by a running script" line, when `held`
    /// differs from what it shows.
    fn show_held(&mut self, held: usize, show: &mut RunPanel<'_>) {
        if held != self.held_shown {
            self.held_shown = held;
            show(&panel_call("setConsoleHeld", &rustkit_engine::devtools_held_text(held)));
        }
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
        self.ask(view, Want::Highlight, DevtoolsQuery::Highlight(path.and_then(devtools_node_of)));
    }

    pub fn start_picker(&mut self, panel: &WebView) {
        self.picking = true;
        run(panel, "window.hiwaveInspector && window.hiwaveInspector.setPickerActive(true)");
    }

    pub fn stop_picker(&mut self, view: &RustKitView, panel: &WebView) {
        self.stop_picker_with(&mut |q| view.devtools_ask(q), &mut |js| run(panel, js));
    }

    fn stop_picker_with(&mut self, send: &mut SendQuestion<'_>, show: &mut RunPanel<'_>) {
        if self.picking {
            self.picking = false;
            self.hovered = None;
            self.hover_next = None;
            self.ask_with(Want::Highlight, DevtoolsQuery::Highlight(None), send);
        }
        show("window.hiwaveInspector && window.hiwaveInspector.setPickerActive(false)");
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
        let deadline = Instant::now() + EVAL_DEADLINE;
        self.asked.push(Asked::Eval(view.devtools_eval(code, deadline), deadline));
    }

    /// Act on the answers that have arrived, then ask for new console
    /// lines when the view has recorded any. Called on every live wake of
    /// the content view.
    pub fn pump(&mut self, view: &RustKitView, panel: &WebView) {
        if !self.open {
            self.asked.clear();
            self.again.clear();
            return;
        }
        let (answers, busy) = self.take_page_answers(Instant::now(), &mut |q| view.devtools_ask(q));
        // Whole seconds, so the line changes once a second at most.
        let busy = busy.map(|waited| waited.as_secs());
        if busy != self.busy_shown {
            self.busy_shown = busy;
            let text = busy.map(|s| format!("page busy (no answer for {s} s)"));
            run(panel, &panel_call("setPageBusy", &text));
        }
        let mut arrived: Vec<Arrived> = answers.into_iter().map(|(want, answer)| Arrived::Page(want, answer)).collect();
        self.asked.retain_mut(|asked| match asked {
            Asked::Page(_) => true,
            Asked::Eval(rx, deadline) => match rx.try_recv() {
                Ok(result) => {
                    arrived.push(Arrived::Eval(result));
                    false
                }
                // Past its deadline the line is given up: the receiver is
                // dropped, so a late answer is discarded, and a page that
                // has not reached it yet skips it (R1 condition 5).
                Err(oneshot::error::TryRecvError::Empty) if Instant::now() >= *deadline => {
                    arrived.push(Arrived::Eval(Err(rustkit_engine::EVAL_CANCELLED.into())));
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
        let inspector = view.devtools_inspector();
        // A page thread that panicked holding the Inspector lock records
        // nothing more; its history still reads, and the panel says so.
        if !self.crash_shown && inspector.as_ref().is_some_and(|i| i.page_crashed()) {
            self.crash_shown = true;
            run(panel, "window.hiwaveInspector && window.hiwaveInspector.addConsoleResult('page crashed', true)");
        }
        // A script that hangs after a hot loop records nothing more; the
        // calls it made past the live cap are only counted (#810 review).
        // One atomic load; the devtools thread wakes the window when it moves.
        let held = inspector.as_ref().map_or(0, |i| i.console_held());
        self.show_held(held, &mut |js| run(panel, js));
        let watermark = inspector.map(|inspector| inspector.watermark());
        if console_due(watermark, self.console_seq, self.waiting_for(Want::Console)) {
            let since = self.console_seq;
            self.ask(view, Want::Console, DevtoolsQuery::Console { since });
        }
    }

    fn hovered_now(&mut self, view: &RustKitView, panel: &WebView, node: Option<usize>) {
        if node == self.hovered {
            return;
        }
        self.hovered = node;
        self.ask(view, Want::Highlight, DevtoolsQuery::Highlight(node));
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
            (Want::Hover, DevtoolsAnswer::Unavailable) => {
                if self.hover_unavailable() {
                    self.hovered_now(view, panel, None);
                }
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
                self.console_answered(console, &mut |q| view.devtools_ask(q), &mut |js| run(panel, js));
            }
            _ => {}
        }
    }

    fn console_answered(&mut self, console: DevtoolsConsole, send: &mut SendQuestion<'_>, show: &mut RunPanel<'_>) {
        if console.next_seq == self.console_seq {
            return;
        }
        self.console_seq = console.next_seq;
        if !console.entries.is_empty() {
            show(&panel_call("addConsoleEntries", &console.entries));
        }
        if console.page_changed {
            self.forget_page();
            self.ask_with(Want::Highlight, DevtoolsQuery::Highlight(None), send);
            show("window.hiwaveInspector && window.hiwaveInspector.setElementStyles(null)");
        }
        // A first layout or the end of the load: the same page, a newer
        // tree, the selection kept.
        if console.tree_stale {
            let reveal = self.selected;
            self.ask_with(Want::Tree, DevtoolsQuery::DomTree { root: None, reveal }, send);
        }
    }
}

/// Sends a question to the page.
type SendQuestion<'a> = dyn FnMut(DevtoolsQuery) -> oneshot::Receiver<DevtoolsAnswer> + 'a;

/// Runs a script in the panel.
type RunPanel<'a> = dyn FnMut(&str) + 'a;

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

    /// A page that answers nothing until the test does: every question sent
    /// and its reply sender, in order.
    #[derive(Default)]
    struct Page {
        sent: Vec<(DevtoolsQuery, oneshot::Sender<DevtoolsAnswer>)>,
    }

    impl Page {
        fn send(&mut self) -> impl FnMut(DevtoolsQuery) -> oneshot::Receiver<DevtoolsAnswer> + '_ {
            |query| {
                let (tx, rx) = oneshot::channel();
                self.sent.push((query, tx));
                rx
            }
        }
    }

    fn tree(reveal: Option<usize>) -> DevtoolsQuery {
        DevtoolsQuery::DomTree { root: None, reveal }
    }

    #[test]
    fn a_busy_page_gets_one_live_question_per_kind_and_late_answers_are_dropped() {
        // T6c: a page in a long script answered every queued question at
        // once when it came back, oldest first, and the panel showed each.
        let mut page = Page::default();
        let mut tools = Devtools::default();
        let now = Instant::now();
        tools.ask_with(Want::Tree, tree(None), &mut page.send());
        tools.ask_with(Want::Tree, tree(Some(4)), &mut page.send());
        tools.ask_with(Want::Tree, tree(Some(9)), &mut page.send());
        tools.ask_with(Want::Element, DevtoolsQuery::Element(9), &mut page.send());
        assert_eq!(page.sent.len(), 2, "a second tree question was sent while one was out");

        let (arrived, busy) = tools.take_page_answers(now, &mut page.send());
        assert!(arrived.is_empty() && busy.is_none(), "busy before the deadline");
        let later = now + LIVE_DEADLINE + Duration::from_secs(1);
        let (_, busy) = tools.take_page_answers(later, &mut page.send());
        assert!(busy.is_some_and(|b| b >= LIVE_DEADLINE), "no busy state past the deadline: {busy:?}");

        // The page comes back and answers the old tree: dropped, and only
        // the newest tree question is asked.
        let (_, tx) = page.sent.remove(0);
        tx.send(DevtoolsAnswer::Tree(None)).unwrap();
        let (arrived, busy) = tools.take_page_answers(later, &mut page.send());
        assert!(arrived.is_empty(), "a late answer was shown: {arrived:?}");
        assert!(busy.is_some(), "the element is still out and late");
        assert_eq!(page.sent.len(), 2);
        assert_eq!(page.sent[1].0, tree(Some(9)), "not the newest tree question");
        // The element too was late, but nothing newer was wanted: shown,
        // not asked again (#809 review 2).
        let (_, element) = page.sent.remove(0);
        element.send(DevtoolsAnswer::Element(None)).unwrap();
        // Questions asked again are stamped now: judge them by the clock.
        let (arrived, _) = tools.take_page_answers(Instant::now(), &mut page.send());
        assert_eq!(arrived, vec![(Want::Element, DevtoolsAnswer::Element(None))]);
        assert_eq!(page.sent.len(), 1, "the element was asked again");

        // Answered in time: shown, and busy clears.
        let (_, tx) = page.sent.remove(0);
        tx.send(DevtoolsAnswer::Tree(None)).unwrap();
        let (arrived, busy) = tools.take_page_answers(Instant::now(), &mut page.send());
        assert_eq!(busy, None, "still busy");
        assert_eq!(arrived, vec![(Want::Tree, DevtoolsAnswer::Tree(None))], "the fresh tree was not shown");
        assert!(page.sent.is_empty(), "nothing more is asked: {}", page.sent.len());
    }

    fn subtree(root: usize) -> DevtoolsQuery {
        DevtoolsQuery::DomTree { root: Some(root), reveal: None }
    }

    #[test]
    fn expanding_two_nodes_while_one_expand_is_out_asks_both() {
        // #809 review 1: Subtree was coalesced per kind, so expanding B and
        // then C while A was out lost B, and B's "more" row did nothing.
        let mut page = Page::default();
        let mut tools = Devtools::default();
        for n in [1, 2, 3] {
            tools.ask_with(Want::Subtree, subtree(n), &mut page.send());
        }
        let sent: Vec<_> = page.sent.iter().map(|(q, _)| q.clone()).collect();
        assert_eq!(sent, vec![subtree(1), subtree(2), subtree(3)], "an expand was lost");
        // The same node twice is one question.
        tools.ask_with(Want::Subtree, subtree(2), &mut page.send());
        assert_eq!(page.sent.len(), 3);
        for (query, tx) in page.sent.drain(..) {
            let DevtoolsQuery::DomTree { root, .. } = query else { unreachable!() };
            tx.send(DevtoolsAnswer::Node(root)).unwrap();
        }
        let (arrived, _) = tools.take_page_answers(Instant::now(), &mut page.send());
        assert_eq!(arrived.len(), 3, "every expand is answered");
        assert_eq!(page.sent.len(), 1, "the repeated expand of 2 is asked once more");
        assert_eq!(page.sent[0].0, subtree(2));
    }

    #[test]
    fn a_page_that_always_answers_late_still_shows_its_answers() {
        // #809 review 2: a page that answers every question at 2.5 s had
        // each answer dropped and the same question asked again, forever.
        let mut page = Page::default();
        let mut tools = Devtools::default();
        let now = Instant::now();
        let at_2_1 = now + Duration::from_millis(2100);
        let at_2_5 = now + Duration::from_millis(2500);
        tools.ask_with(Want::Tree, tree(None), &mut page.send());
        tools.ask_with(Want::Element, DevtoolsQuery::Element(4), &mut page.send());
        let (_, busy) = tools.take_page_answers(at_2_1, &mut page.send());
        assert!(busy.is_some(), "not late at 2.1 s");
        for (query, tx) in page.sent.drain(..) {
            let answer = match query {
                DevtoolsQuery::Element(_) => DevtoolsAnswer::Element(None),
                _ => DevtoolsAnswer::Tree(None),
            };
            tx.send(answer).unwrap();
        }
        let (arrived, busy) = tools.take_page_answers(at_2_5, &mut page.send());
        assert_eq!(
            arrived,
            vec![(Want::Tree, DevtoolsAnswer::Tree(None)), (Want::Element, DevtoolsAnswer::Element(None))],
            "late answers with nothing newer wanted were not shown"
        );
        assert!(page.sent.is_empty(), "the same question was asked again: {:?}", page.sent.iter().map(|s| &s.0).collect::<Vec<_>>());
        assert_eq!(busy, None);
    }

    #[test]
    fn a_late_hover_after_the_pointer_left_clears_the_outline() {
        // #809 review 3: the pointer left the view while a hover was late;
        // the late answer was dropped and the outline stayed.
        let mut page = Page::default();
        let mut tools = Devtools::default();
        let later = Instant::now() + LIVE_DEADLINE + Duration::from_secs(1);
        tools.ask_with(Want::Hover, DevtoolsQuery::NodeAt { x: 1.0, y: 1.0 }, &mut page.send());
        let (_, busy) = tools.take_page_answers(later, &mut page.send());
        assert!(busy.is_some(), "the hover is not late");
        tools.hover_next = Some(None);
        let (_, tx) = page.sent.remove(0);
        tx.send(DevtoolsAnswer::Node(Some(7))).unwrap();
        let (arrived, _) = tools.take_page_answers(later, &mut page.send());
        assert_eq!(arrived, vec![(Want::Hover, DevtoolsAnswer::Node(None))], "the outline is not cleared");
        assert_eq!(tools.hover_next, None);
        assert!(page.sent.is_empty());
    }

    #[test]
    fn an_unavailable_hover_after_the_pointer_left_clears_the_outline() {
        let mut tools = Devtools { picking: true, ..Default::default() };
        tools.hover_next = Some(None);
        assert!(tools.hover_unavailable(), "the outline stays");
        assert_eq!(tools.hover_next, None);
        tools.hover_next = Some(Some((1.0, 2.0)));
        assert!(!tools.hover_unavailable(), "a pointer still in the view cleared the outline");
        // As before: nothing more is asked of a page that could not answer.
        assert_eq!(tools.hover_next, None);
        assert!(!tools.hover_unavailable());
    }

    #[test]
    fn a_navigation_forgets_live_questions_and_a_close_forgets_all() {
        // #809 follow-up: through the console answer that says the page
        // changed, and through close(), not the helpers they call.
        let mut page = Page::default();
        let mut shown = Vec::<String>::new();
        let mut tools = Devtools { open: true, ..Default::default() };
        let now = Instant::now();
        tools.selected = Some(3);
        tools.ask_with(Want::Tree, tree(Some(3)), &mut page.send());
        tools.ask_with(Want::Tree, tree(Some(5)), &mut page.send());
        tools.take_page_answers(now + LIVE_DEADLINE, &mut page.send());
        let old_page: Vec<_> = page.sent.drain(..).collect();

        let changed = DevtoolsConsole { next_seq: 1, page_changed: true, ..Default::default() };
        tools.console_answered(changed, &mut page.send(), &mut |js| shown.push(js.into()));
        assert_eq!(tools.selected, None);
        assert!(tools.again.is_empty(), "a question about the old page waits to be asked");
        assert_eq!(page.sent.len(), 1, "the outline is not cleared");
        assert_eq!(page.sent[0].0, DevtoolsQuery::Highlight(None));
        assert!(shown.iter().any(|js| js.contains("setElementStyles(null)")), "{shown:?}");
        // The old page's tree comes back: not shown, nothing asked, not busy.
        for (_, tx) in old_page {
            let _ = tx.send(DevtoolsAnswer::Tree(None));
        }
        page.sent.clear();
        let (arrived, busy) = tools.take_page_answers(now + LIVE_DEADLINE, &mut page.send());
        assert!(arrived.iter().all(|(w, _)| *w == Want::Highlight), "an old page's answer was shown: {arrived:?}");
        assert!(page.sent.is_empty());
        assert_eq!(busy, None);

        tools.ask_with(Want::Tree, tree(None), &mut page.send());
        tools.ask_with(Want::Tree, tree(Some(1)), &mut page.send());
        tools.busy_shown = Some(3);
        let before_close = page.sent.len();
        tools.close_with(&mut page.send(), &mut |js| shown.push(js.into()));
        assert!(tools.again.is_empty() && tools.busy_shown.is_none());
        // Only the close's own outline and recording switch are out.
        let out: Vec<Want> = tools
            .asked
            .iter()
            .filter_map(|a| match a {
                Asked::Page(q) => Some(q.want),
                Asked::Eval(..) => None,
            })
            .collect();
        assert_eq!(out, vec![Want::Highlight, Want::Ack], "a question from before the close is still out");
        assert_eq!(page.sent.len(), before_close + 2);
    }

    #[test]
    fn clearing_the_console_brings_the_held_line_back_on_the_next_pump() {
        // #809 follow-up: the panel's clearConsole() hides the held line,
        // and the next pump, with the count unchanged, did not show it again.
        let mut tools = Devtools::default();
        let mut shown = Vec::<String>::new();
        tools.show_held(4, &mut |js| shown.push(js.into()));
        assert_eq!(shown.len(), 1);
        tools.show_held(4, &mut |js| shown.push(js.into()));
        assert_eq!(shown.len(), 1, "an unchanged count is shown again");
        tools.clear_console_with(&mut |js| shown.push(js.into()));
        assert!(shown[1].contains("clearConsole()"));
        tools.show_held(4, &mut |js| shown.push(js.into()));
        assert_eq!(shown.len(), 3, "the held line stays hidden while 4 lines are held");
        assert!(shown[2].contains("setConsoleHeld"), "{shown:?}");
        // Nothing held: a clear leaves nothing to show.
        tools.show_held(0, &mut |js| shown.push(js.into()));
        tools.clear_console_with(&mut |js| shown.push(js.into()));
        tools.show_held(0, &mut |js| shown.push(js.into()));
        assert!(!shown.last().unwrap().contains("setConsoleHeld"), "{shown:?}");
    }

    #[test]
    fn a_question_answered_in_time_asks_what_was_wanted_meanwhile() {
        let mut page = Page::default();
        let mut tools = Devtools::default();
        tools.ask_with(Want::Element, DevtoolsQuery::Element(1), &mut page.send());
        tools.ask_with(Want::Element, DevtoolsQuery::Element(2), &mut page.send());
        tools.ask_with(Want::Element, DevtoolsQuery::Element(3), &mut page.send());
        let (_, tx) = page.sent.remove(0);
        tx.send(DevtoolsAnswer::Element(None)).unwrap();
        let (arrived, _) = tools.take_page_answers(Instant::now(), &mut page.send());
        assert_eq!(arrived.len(), 1);
        assert_eq!(page.sent.len(), 1);
        assert_eq!(page.sent[0].0, DevtoolsQuery::Element(3), "the newest element is asked next");
    }

    #[test]
    fn console_reads_and_the_recording_switch_have_no_deadline() {
        let mut page = Page::default();
        let mut tools = Devtools::default();
        tools.ask_with(Want::Ack, DevtoolsQuery::Attach(true), &mut page.send());
        tools.ask_with(Want::Ack, DevtoolsQuery::Attach(false), &mut page.send());
        tools.ask_with(Want::Console, DevtoolsQuery::Console { since: 0 }, &mut page.send());
        assert_eq!(page.sent.len(), 3, "the switch must reach the page every time");
        let (_, busy) = tools.take_page_answers(Instant::now() + Duration::from_secs(60), &mut page.send());
        assert_eq!(busy, None);
    }

    #[test]
    fn an_open_panel_asks_for_the_console_only_when_the_view_recorded() {
        // #785 R1 HOLD 1: asking again on every answer, while every answer
        // woke the window, kept an idle page and an open panel waking each
        // other. T6b: no clock at all; an answer that brought nothing new
        // leaves the watermark where the panel has read, so it is not asked
        // again however often the window wakes.
        assert!(console_due(Some(1), 0, false), "a new record is asked for");
        assert!(!console_due(Some(5), 5, false), "nothing new, nothing asked");
        assert!(!console_due(Some(9), 5, true), "one question out at a time");
        assert!(!console_due(None, 0, false), "no view, nothing asked");
    }
}
