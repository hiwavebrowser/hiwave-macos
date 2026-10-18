//! `HIWAVE_DUMP_RENDER=<path>` writes a capped picture of `main#main-content`
//! after the large feed insertion, and again when the load is quiet.
//! `HIWAVE_DUMP_RENDER_AT_MS` also writes one snapshot that many milliseconds
//! after navigation start.
//!
//! The file is `{ "dumps": [ ... ] }`. The same object is copied onto the
//! page trace as `render` (the latest dump) so `diff_trace.py` can compare
//! it with the Chrome trace. Nothing here changes layout or paint.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use rustkit_dom::{is_valid_custom_element_name, Document, Node, ShadowRootMode};
use rustkit_layout::{LayoutBox, PaintCapture, Rect};
use serde_json::{json, Value};

use crate::page_trace;

/// Commits recorded after the large `MAIN` insertion, before quiet.
const DUMPS_AFTER_INSERT: u32 = 3;
const MAX_DUMPS: usize = 12;
const POSTS: usize = 3;

struct State {
    path: Option<PathBuf>,
    at_ms: Option<u64>,
    started: Option<Instant>,
    inserts_pending: u32,
    at_dumped: bool,
    quiet_dumped: bool,
    latest: Option<Value>,
    dumps: Vec<Value>,
}

impl State {
    fn new() -> Self {
        Self {
            path: None,
            at_ms: None,
            started: None,
            inserts_pending: 0,
            at_dumped: false,
            quiet_dumped: false,
            latest: None,
            dumps: Vec::new(),
        }
    }
}

std::thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::new());
    static FORCE_PATH: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    static FORCE_AT: RefCell<Option<Option<u64>>> = const { RefCell::new(None) };
}

pub struct ForceDump {
    _private: (),
}

impl Drop for ForceDump {
    fn drop(&mut self) {
        FORCE_PATH.with(|slot| *slot.borrow_mut() = None);
        FORCE_AT.with(|slot| *slot.borrow_mut() = None);
        STATE.with(|slot| *slot.borrow_mut() = State::new());
    }
}

pub fn force_for_test(path: &Path) -> ForceDump {
    FORCE_PATH.with(|slot| *slot.borrow_mut() = Some(path.to_path_buf()));
    STATE.with(|slot| {
        let mut state = slot.borrow_mut();
        state.path = Some(path.to_path_buf());
        state.started = Some(Instant::now());
    });
    ForceDump { _private: () }
}

pub(crate) fn enabled() -> bool {
    path().is_some()
}

pub(crate) fn dump_at_ms() -> Option<u64> {
    if let Some(forced) = FORCE_AT.with(|slot| *slot.borrow()) {
        return forced;
    }
    std::env::var("HIWAVE_DUMP_RENDER_AT_MS")
        .ok()
        .and_then(|value| value.trim().parse().ok())
}

fn path() -> Option<PathBuf> {
    if let Some(path) = FORCE_PATH.with(|slot| slot.borrow().clone()) {
        return Some(path);
    }
    let from_state = STATE.with(|slot| slot.borrow().path.clone());
    if from_state.is_some() {
        return from_state;
    }
    std::env::var("HIWAVE_DUMP_RENDER")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// Read the env once per load. The mutation hook is held until quiet so a
/// yielded load still sees the feed insertion after `load_url` returns.
pub(crate) struct Watch {
    active: bool,
}

impl Drop for Watch {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        if page_trace::is_parked() {
            return;
        }
        write_quiet_if_due();
        if page_trace::dump_held() {
            page_trace::release_hook();
            page_trace::set_dump_held(false);
        }
    }
}

pub(crate) fn watch() -> Watch {
    let Some(path) = path() else {
        return Watch { active: false };
    };
    STATE.with(|slot| {
        let mut state = slot.borrow_mut();
        state.path = Some(path);
        state.at_ms = dump_at_ms();
        state.started = Some(Instant::now());
        state.inserts_pending = 0;
        state.at_dumped = false;
        state.quiet_dumped = false;
        state.latest = None;
        state.dumps.clear();
    });
    page_trace::hold_hook();
    page_trace::set_dump_held(true);
    Watch { active: true }
}

pub(crate) fn note_main_insert() {
    if !enabled() {
        return;
    }
    STATE.with(|slot| {
        let mut state = slot.borrow_mut();
        if state.dumps.len() >= MAX_DUMPS {
            return;
        }
        state.inserts_pending = DUMPS_AFTER_INSERT;
    });
}

pub struct PaintGuard {
    armed: bool,
    paint: Option<PaintCapture>,
}

impl PaintGuard {
    pub(crate) fn arm(main: Option<usize>) -> Self {
        if enabled() {
            rustkit_layout::begin_paint_capture(main);
            Self {
                armed: true,
                paint: None,
            }
        } else {
            Self {
                armed: false,
                paint: None,
            }
        }
    }

    pub(crate) fn take(&mut self) -> Option<PaintCapture> {
        if !self.armed {
            return None;
        }
        self.armed = false;
        self.paint = rustkit_layout::end_paint_capture();
        self.paint.take()
    }
}

impl Drop for PaintGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = rustkit_layout::end_paint_capture();
        }
    }
}

pub(crate) fn main_node(document: &Document) -> Option<usize> {
    document
        .get_element_by_id("main-content")
        .filter(|node| node.tag_name() == Some("main"))
        .map(|node| node.id.raw())
}

pub(crate) fn after_commit(document: &Document, layout: &LayoutBox, paint: Option<&PaintCapture>) {
    if !enabled() {
        page_trace::poll_settle();
        return;
    }
    let t_ms = elapsed_ms();
    let at_ms = dump_at_ms();
    let pending = STATE.with(|slot| slot.borrow().inserts_pending);
    let at_due = STATE.with(|slot| {
        let state = slot.borrow();
        at_ms.is_some_and(|ms| t_ms >= ms) && !state.at_dumped
    });
    let mut reasons = Vec::new();
    if pending > 0 {
        reasons.push("after-main-insert");
    }
    if at_due {
        reasons.push("at-ms");
    }
    let snapshot = build_snapshot(document, layout, paint, "latest", t_ms);
    STATE.with(|slot| slot.borrow_mut().latest = Some(snapshot.clone()));
    for reason in reasons {
        push_dump(retarget(&snapshot, reason));
        if reason == "after-main-insert" {
            STATE.with(|slot| {
                let pending = &mut slot.borrow_mut().inserts_pending;
                *pending = pending.saturating_sub(1);
            });
        }
        if reason == "at-ms" {
            STATE.with(|slot| slot.borrow_mut().at_dumped = true);
        }
    }
    page_trace::poll_settle();
}

pub(crate) fn write_quiet_if_due() {
    if !enabled() {
        return;
    }
    let already = STATE.with(|slot| slot.borrow().quiet_dumped);
    if already {
        return;
    }
    let t_ms = elapsed_ms();
    let snapshot = STATE.with(|slot| {
        slot.borrow().latest.clone().unwrap_or_else(|| {
            json!({
                "t_ms": t_ms,
                "when": "quiet",
                "display_list_items_in_main": 0,
                "main": null,
                "note": "no frame was committed"
            })
        })
    });
    push_dump(retarget(&snapshot, "quiet"));
    STATE.with(|slot| slot.borrow_mut().quiet_dumped = true);
}

fn retarget(snapshot: &Value, when: &str) -> Value {
    let mut snapshot = snapshot.clone();
    if let Some(object) = snapshot.as_object_mut() {
        object.insert("when".into(), json!(when));
    }
    snapshot
}

fn push_dump(snapshot: Value) {
    let path = STATE.with(|slot| {
        let mut state = slot.borrow_mut();
        if state.dumps.len() >= MAX_DUMPS {
            return None;
        }
        state.dumps.push(snapshot.clone());
        state.path.clone()
    });
    page_trace::set_render(snapshot);
    if let Some(path) = path {
        let body = STATE.with(|slot| {
            let dumps = slot.borrow().dumps.clone();
            serde_json::to_string_pretty(&json!({ "dumps": dumps })).unwrap_or_else(|_| "{}".into())
        });
        write_atomic(&path, &body);
    }
}

fn elapsed_ms() -> u64 {
    if let Some(ms) = page_trace::elapsed_ms() {
        return ms;
    }
    STATE.with(|slot| {
        slot.borrow()
            .started
            .map(|started| started.elapsed().as_millis() as u64)
            .unwrap_or(0)
    })
}

fn write_atomic(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(parent);
        }
    }
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, body).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

struct Placed<'a> {
    boxes: Vec<&'a LayoutBox>,
    clipped: bool,
}

fn build_snapshot(
    document: &Document,
    layout: &LayoutBox,
    paint: Option<&PaintCapture>,
    when: &str,
    t_ms: u64,
) -> Value {
    let painted: HashSet<usize> = paint
        .map(|paint| paint.painted.iter().copied().collect())
        .unwrap_or_default();
    let mut included = HashSet::new();
    mark_included(layout, &painted, &mut included);
    let mut placed = HashMap::new();
    index_boxes(layout, None, &mut placed);
    let commands = paint.map(|paint| paint.commands_in_main).unwrap_or(0);
    let defined = document.defined_custom_elements();
    let Some(main) = document
        .get_element_by_id("main-content")
        .filter(|node| node.tag_name() == Some("main"))
    else {
        return json!({
            "t_ms": t_ms,
            "when": when,
            "display_list_items_in_main": commands,
            "main": null,
            "note": "no main#main-content"
        });
    };
    let posts = first_posts(&main);
    let mut keep: HashSet<usize> = HashSet::new();
    keep.insert(main.id.raw());
    for post in &posts {
        let mut cursor = Some(post.clone());
        while let Some(node) = cursor {
            keep.insert(node.id.raw());
            if node.id == main.id {
                break;
            }
            cursor = if node.is_shadow_root() {
                node.shadow_host()
            } else {
                node.parent()
            };
        }
    }
    json!({
        "t_ms": t_ms,
        "when": when,
        "display_list_items_in_main": commands,
        "main": element_json(&main, &defined, &placed, &included, &keep, &posts, true)
    })
}

fn mark_included(
    box_: &LayoutBox,
    painted: &HashSet<usize>,
    included: &mut HashSet<usize>,
) -> bool {
    let mut yes = box_.node_id.is_some_and(|id| painted.contains(&id));
    for child in &box_.children {
        if mark_included(child, painted, included) {
            yes = true;
        }
    }
    if yes {
        if let Some(id) = box_.node_id {
            included.insert(id);
        }
    }
    yes
}

fn index_boxes<'a>(box_: &'a LayoutBox, clip: Option<Rect>, out: &mut HashMap<usize, Placed<'a>>) {
    let border = box_.dimensions.border_box();
    let clipped = clip.is_some_and(|clip| !intersects(clip, border));
    if let Some(id) = box_.node_id {
        let place = out.entry(id).or_insert_with(|| Placed {
            boxes: Vec::new(),
            clipped: false,
        });
        place.boxes.push(box_);
        place.clipped |= clipped;
    }
    let child_clip = if clips_overflow(box_) {
        Some(match clip {
            Some(clip) => intersection(clip, box_.dimensions.padding_box()),
            None => box_.dimensions.padding_box(),
        })
    } else {
        clip
    };
    for child in &box_.children {
        index_boxes(child, child_clip, out);
    }
}

fn clips_overflow(box_: &LayoutBox) -> bool {
    box_.style.overflow_x != rustkit_css::Overflow::Visible
        || box_.style.overflow_y != rustkit_css::Overflow::Visible
}

fn intersects(a: Rect, b: Rect) -> bool {
    a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
}

fn intersection(a: Rect, b: Rect) -> Rect {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    Rect {
        x,
        y,
        width: (a.x + a.width).min(b.x + b.width) - x,
        height: (a.y + a.height).min(b.y + b.height) - y,
    }
}

fn first_posts(main: &std::rc::Rc<Node>) -> Vec<std::rc::Rc<Node>> {
    let mut posts = Vec::new();
    fn walk(node: &std::rc::Rc<Node>, posts: &mut Vec<std::rc::Rc<Node>>) {
        if posts.len() >= POSTS {
            return;
        }
        if node.tag_name() == Some("shreddit-post") {
            posts.push(node.clone());
        }
        if let Some(root) = node.shadow_root() {
            for child in root.children() {
                walk(&child, posts);
            }
        }
        for child in node.children() {
            if child.is_element() {
                walk(&child, posts);
            }
        }
    }
    walk(main, &mut posts);
    posts
}

fn element_json(
    node: &std::rc::Rc<Node>,
    defined: &std::collections::HashSet<String>,
    placed: &HashMap<usize, Placed<'_>>,
    included: &HashSet<usize>,
    keep: &HashSet<usize>,
    posts: &[std::rc::Rc<Node>],
    descend: bool,
) -> Value {
    let tag = node.tag_name().unwrap_or("").to_string();
    let id = node.get_attribute("id").unwrap_or("").to_string();
    let class = node.get_attribute("class").unwrap_or("").to_string();
    let place = placed.get(&node.id.raw());
    let box_value = match place.and_then(|place| place.boxes.first()) {
        Some(box_) => {
            let border = box_.dimensions.border_box();
            json!({
                "x": round_px(border.x),
                "y": round_px(border.y),
                "width": round_px(border.width),
                "height": round_px(border.height),
            })
        }
        None => json!("no box"),
    };
    let style = style_json(node, place);
    let skip = skip_reason(node, place, &style);
    let on_path = keep.contains(&node.id.raw());
    let is_post = posts.iter().any(|post| post.id == node.id);
    let mut children = Vec::new();
    if descend && on_path {
        let mut seen = HashSet::new();
        let mut push_child = |child: &std::rc::Rc<Node>, children: &mut Vec<Value>| {
            if !child.is_element() || !seen.insert(child.id.raw()) {
                return;
            }
            let child_on_path = keep.contains(&child.id.raw());
            let child_descend = child_on_path && !is_post;
            children.push(element_json(
                child,
                defined,
                placed,
                included,
                keep,
                posts,
                child_descend,
            ));
        };
        if let Some(root) = node.shadow_root() {
            for child in root.children() {
                push_child(&child, &mut children);
            }
        }
        for child in node.children() {
            push_child(&child, &mut children);
        }
    }
    json!({
        "tag": tag,
        "id": id,
        "class": class,
        "custom_element": custom_state(&tag, defined),
        "hidden": node.get_attribute("hidden").is_some(),
        "shadow": shadow_json(node),
        "slot": slot_json(node),
        "style": style,
        "box": box_value,
        "in_display_list": included.contains(&node.id.raw()),
        "skip": skip,
        "children": children,
    })
}

fn custom_state(tag: &str, defined: &std::collections::HashSet<String>) -> &'static str {
    if !is_valid_custom_element_name(tag) {
        return "uncustomized";
    }
    if defined.contains(tag) {
        "defined"
    } else {
        // Settled names are still undefined to script. There is no
        // failed-upgrade bit to report `failed`.
        "undefined"
    }
}

fn shadow_json(node: &Node) -> Value {
    let Some(root) = node.shadow_root() else {
        return Value::Null;
    };
    let mode = match node.shadow_root_init().map(|init| init.mode) {
        Some(ShadowRootMode::Closed) => "closed",
        _ => "open",
    };
    json!({
        "mode": mode,
        "child_count": root.children().len(),
        "adopted_stylesheets": 0
    })
}

fn slot_json(node: &std::rc::Rc<Node>) -> Value {
    let assigned = node.assigned_slot();
    let assigned_to = assigned
        .as_ref()
        .map(|slot| slot.get_attribute("name").unwrap_or("").to_string());
    let name = node.get_attribute("slot").map(str::to_string);
    // `assigned_nodes` needs an `Rc` (it walks the host's children).
    let assigned_count = node.is_slot().then(|| node.assigned_nodes().len());
    if name.is_none() && assigned_to.is_none() && assigned_count.is_none() {
        return Value::Null;
    }
    json!({
        "name": name,
        "assigned_to": assigned_to,
        "assigned_count": assigned_count,
    })
}

fn style_json(node: &Node, place: Option<&Placed<'_>>) -> Value {
    let content_visibility = inline_content_visibility(node);
    let Some(box_) = place.and_then(|place| place.boxes.first().copied()) else {
        return json!({
            "display": Value::Null,
            "visibility": Value::Null,
            "opacity": Value::Null,
            "content-visibility": content_visibility,
            "position": Value::Null,
            "height": Value::Null,
            "overflow": Value::Null,
        });
    };
    let style = &*box_.style;
    let border = box_.dimensions.border_box();
    json!({
        "display": css_display(style.display),
        "visibility": css_visibility(style.visibility),
        "opacity": round_px(style.opacity),
        "content-visibility": content_visibility,
        "position": css_position(style.position),
        "height": format!("{}px", round_px(border.height)),
        "overflow": css_overflow(style.overflow_x, style.overflow_y),
    })
}

fn skip_reason(node: &Node, place: Option<&Placed<'_>>, style: &Value) -> Option<&'static str> {
    if unslotted(node) {
        return Some("unslotted light DOM");
    }
    if style.get("display").and_then(Value::as_str) == Some("none") {
        return Some("display:none");
    }
    if style.get("content-visibility").and_then(Value::as_str) == Some("hidden") {
        return Some("content-visibility");
    }
    let Some(place) = place else {
        return Some("no box");
    };
    let Some(box_) = place.boxes.first() else {
        return Some("no box");
    };
    let border = box_.dimensions.border_box();
    if border.width <= 0.0 && border.height <= 0.0 {
        return Some("zero size");
    }
    if place.clipped {
        return Some("clipped");
    }
    None
}

fn unslotted(node: &Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    parent.shadow_root().is_some() && node.assigned_slot().is_none()
}

fn inline_content_visibility(node: &Node) -> String {
    let Some(style) = node.get_attribute("style") else {
        return "visible".into();
    };
    for decl in style.split(';') {
        let mut parts = decl.splitn(2, ':');
        if parts
            .next()
            .is_some_and(|name| name.trim().eq_ignore_ascii_case("content-visibility"))
        {
            let value = parts.next().unwrap_or("").trim();
            if !value.is_empty() {
                return value.to_string();
            }
        }
    }
    "visible".into()
}

fn css_display(display: rustkit_css::Display) -> &'static str {
    use rustkit_css::Display;
    match display {
        Display::Block => "block",
        Display::Inline => "inline",
        Display::InlineBlock => "inline-block",
        Display::Flex => "flex",
        Display::InlineFlex => "inline-flex",
        Display::Grid => "grid",
        Display::InlineGrid => "inline-grid",
        Display::None => "none",
        Display::Table => "table",
        Display::InlineTable => "inline-table",
        Display::TableRowGroup => "table-row-group",
        Display::TableHeaderGroup => "table-header-group",
        Display::TableFooterGroup => "table-footer-group",
        Display::TableRow => "table-row",
        Display::TableCell => "table-cell",
        Display::TableCaption => "table-caption",
        Display::TableColumn => "table-column",
        Display::TableColumnGroup => "table-column-group",
        Display::Contents => "contents",
    }
}

fn css_visibility(visibility: rustkit_css::Visibility) -> &'static str {
    match visibility {
        rustkit_css::Visibility::Visible => "visible",
        rustkit_css::Visibility::Hidden => "hidden",
        rustkit_css::Visibility::Collapse => "collapse",
    }
}

fn css_position(position: rustkit_css::Position) -> &'static str {
    match position {
        rustkit_css::Position::Static => "static",
        rustkit_css::Position::Relative => "relative",
        rustkit_css::Position::Absolute => "absolute",
        rustkit_css::Position::Fixed => "fixed",
        rustkit_css::Position::Sticky => "sticky",
    }
}

fn css_overflow(x: rustkit_css::Overflow, y: rustkit_css::Overflow) -> String {
    let x = css_overflow_one(x);
    let y = css_overflow_one(y);
    if x == y {
        x.to_string()
    } else {
        format!("{x} {y}")
    }
}

fn css_overflow_one(overflow: rustkit_css::Overflow) -> &'static str {
    match overflow {
        rustkit_css::Overflow::Visible => "visible",
        rustkit_css::Overflow::Hidden => "hidden",
        rustkit_css::Overflow::Scroll => "scroll",
        rustkit_css::Overflow::Auto => "auto",
        rustkit_css::Overflow::Clip => "clip",
    }
}

fn round_px(value: f32) -> f64 {
    (value as f64 * 100.0).round() / 100.0
}
