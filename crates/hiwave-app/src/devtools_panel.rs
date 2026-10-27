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
use rustkit_engine::{devtools_node_of, devtools_path, DevtoolsAnswer, DevtoolsQuery};
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
        if want.live() && self.waiting_for(want) {
            self.again.retain(|(w, _)| *w != want);
            self.again.push((want, query));
            return;
        }
        let rx = send(query.clone());
        self.asked.push(Asked::Page(Question { want, query, rx, asked_at: Instant::now(), late: false }));
    }

    fn waiting_for(&self, want: Want) -> bool {
        self.asked.iter().any(|a| matches!(a, Asked::Page(q) if q.want == want))
    }

    /// The page's answers that have arrived, in the order asked. A live
    /// question past its deadline is marked late; a late answer is dropped
    /// and its kind asked again with the newest question wanted (the
    /// pointer's newest position for a hover). Returns how long the oldest
    /// late question has waited, if one is out: the page is busy.
    fn take_page_answers(&mut self, now: Instant, send: &mut SendQuestion<'_>) -> (Vec<(Want, DevtoolsAnswer)>, Option<Duration>) {
        let mut arrived = Vec::new();
        let mut freed = Vec::new();
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
                freed.push((q.want, q.query.clone()));
            } else {
                arrived.push((q.want, answer));
            }
            false
        });
        for (want, query) in freed {
            let newest = match want {
                Want::Hover => match self.hover_next.take() {
                    Some(Some((x, y))) => Some(DevtoolsQuery::NodeAt { x, y }),
                    _ => None,
                },
                _ => Some(self.take_again(want).unwrap_or(query)),
            };
            if let Some(query) = newest {
                self.ask_with(want, query, send);
            }
        }
        // Answered in time: what was wanted meanwhile is asked now.
        for want in arrived.iter().map(|(w, _)| *w).collect::<Vec<_>>() {
            if !self.waiting_for(want) {
                if let Some(query) = self.take_again(want) {
                    self.ask_with(want, query, send);
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

    fn take_again(&mut self, want: Want) -> Option<DevtoolsQuery> {
        let at = self.again.iter().position(|(w, _)| *w == want)?;
        Some(self.again.remove(at).1)
    }

    /// The panel opened: record the page's console from now on, and send
    /// the DOM tree and what the console holds.
    pub fn open(&mut self, view: &RustKitView, _panel: &WebView) {
        self.open = true;
        self.console_seq = 0;
        self.crash_shown = false;
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
        self.again.clear();
        self.busy_shown = None;
        run(panel, "window.hiwaveInspector && window.hiwaveInspector.setPageBusy(null)");
        self.ask(view, Want::Highlight, DevtoolsQuery::Highlight(None));
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
        self.ask(view, Want::Highlight, DevtoolsQuery::Highlight(path.and_then(devtools_node_of)));
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
            self.ask(view, Want::Highlight, DevtoolsQuery::Highlight(None));
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
            (Want::Hover, DevtoolsAnswer::Unavailable) => self.hover_next = None,
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
                // its selection and outline, and an answer still out about
                // the old page's nodes is not shown on the new one.
                if console.page_changed {
                    self.selected = None;
                    self.hovered = None;
                    self.hover_next = None;
                    self.asked.retain(|a| match a {
                        Asked::Eval(..) => true,
                        Asked::Page(q) => !q.want.live(),
                    });
                    self.again.clear();
                    self.ask(view, Want::Highlight, DevtoolsQuery::Highlight(None));
                    run(panel, "window.hiwaveInspector && window.hiwaveInspector.setElementStyles(null)");
                }
                // A first layout or the end of the load: the same page,
                // a newer tree, the selection kept.
                if console.tree_stale {
                    self.send_tree(view);
                }
            }
            _ => {}
        }
    }
}

/// Sends a question to the page.
type SendQuestion<'a> = dyn FnMut(DevtoolsQuery) -> oneshot::Receiver<DevtoolsAnswer> + 'a;

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
        // The element too was late: dropped, asked again.
        let (_, element) = page.sent.remove(0);
        element.send(DevtoolsAnswer::Element(None)).unwrap();
        // Questions asked again are stamped now: judge them by the clock.
        let (arrived, _) = tools.take_page_answers(Instant::now(), &mut page.send());
        assert!(arrived.is_empty(), "a late element was shown: {arrived:?}");
        assert_eq!(page.sent.len(), 2);
        assert_eq!(page.sent[1].0, DevtoolsQuery::Element(9));

        // Answered in time: shown, and busy clears.
        for (query, tx) in page.sent.drain(..) {
            let answer = match query {
                DevtoolsQuery::Element(_) => DevtoolsAnswer::Element(None),
                _ => DevtoolsAnswer::Tree(None),
            };
            tx.send(answer).unwrap();
        }
        let (arrived, busy) = tools.take_page_answers(Instant::now(), &mut page.send());
        assert_eq!(busy, None, "still busy");
        assert_eq!(
            arrived,
            vec![(Want::Tree, DevtoolsAnswer::Tree(None)), (Want::Element, DevtoolsAnswer::Element(None))],
            "the fresh answers were not shown"
        );
        assert!(page.sent.is_empty(), "nothing more is asked: {}", page.sent.len());
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
