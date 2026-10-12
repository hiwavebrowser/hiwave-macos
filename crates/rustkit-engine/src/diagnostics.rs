//! Element diagnostics: one compact, deterministic record per element.
//!
//! When a page renders wrong, the first two questions are WHICH element and
//! WHICH stage (DOM, style, layout, paint). This module answers them for one
//! element from what the engine already holds after a layout: the DOM, the
//! committed layout tree (every box carries its node id and computed style)
//! and the view's scroll and viewport. `tools/inspector` writes the same
//! record from Chromium and names the first element whose record differs.
//!
//! Read-only and on demand. Nothing here runs during layout or paint: the
//! per-box context (containing block, clip, opacity chain) is rebuilt from
//! the committed tree each time a record is asked for, so a page that asks
//! for no diagnostics pays nothing. It runs no script and borrows only the
//! view's last committed layout and DOM, so it can answer while the page's
//! script is busy (see `Engine::element_diagnostics`).
//!
//! A field the engine cannot answer is `null` with a reason under
//! `unavailable`, keyed by the field's path; nothing is silently omitted.
//! Maps are `BTreeMap`s so key order is stable; there are no timestamps.

use super::*;
use rustkit_css::{Display, GridLine, Length, Overflow, TrackSize};
use serde::Serialize;
use std::collections::BTreeMap;

/// Version tag carried by every record and dump.
pub const DIAGNOSTICS_SCHEMA: &str = "hiwave-element-diagnostics/1";

/// Default cap on the number of records in a whole-page dump.
pub const DEFAULT_MAX_ELEMENTS: usize = 5000;

/// The computed properties a record carries, in the names `getComputedStyle`
/// uses. `tools/inspector` reads the same list in Chromium.
pub const STYLE_PROPERTIES: &[&str] = &[
    "align-items", "align-self", "aspect-ratio", "background-color", "background-image",
    "border-bottom-width", "border-left-width", "border-right-width", "border-top-width",
    "bottom", "box-sizing", "clear", "color", "column-gap", "contain", "content-visibility",
    "display", "flex-basis", "flex-direction", "flex-grow", "flex-shrink", "flex-wrap",
    "float", "font-family", "font-size", "font-weight", "gap", "grid-auto-columns",
    "grid-auto-flow", "grid-auto-rows", "grid-column-end", "grid-column-start",
    "grid-row-end", "grid-row-start", "grid-template-columns", "grid-template-rows",
    "height", "inset", "justify-content", "justify-self", "left", "line-height",
    "margin-bottom", "margin-left", "margin-right", "margin-top", "max-height", "max-width",
    "min-height", "min-width", "object-fit", "opacity", "order", "overflow-x", "overflow-y",
    "padding-bottom", "padding-left", "padding-right", "padding-top", "position", "right",
    "row-gap", "text-align", "top", "transform", "vertical-align", "visibility",
    "white-space", "width", "z-index",
];

/// Which element to describe.
#[derive(Debug, Clone, PartialEq)]
pub enum DiagnosticTarget {
    /// A raw DOM `NodeId`.
    Node(usize),
    /// The first element in document order matching a CSS selector.
    Selector(String),
    /// The element hit at a point, in viewport CSS px.
    Point { x: f32, y: f32 },
}

/// How a record is built.
#[derive(Debug, Clone)]
pub struct DiagnosticsOptions {
    /// Keep the page URL's query and fragment (stripped by default: they can
    /// carry tokens).
    pub include_url_query: bool,
    /// Cap for `Engine::all_element_diagnostics`.
    pub max_elements: usize,
}

impl Default for DiagnosticsOptions {
    fn default() -> Self {
        Self { include_url_query: false, max_elements: DEFAULT_MAX_ELEMENTS }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct DiagRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EngineBuild {
    pub version: String,
    /// `HIWAVE_BUILD_COMMIT` at compile time, when the build set it.
    pub commit: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PageInfo {
    pub url: Option<String>,
    pub engine: EngineBuild,
    pub viewport: (u32, u32),
    pub device_scale: Option<f32>,
    pub scroll: (f32, f32),
    /// False on builds whose text shaper is the fixed-advance stub (Linux):
    /// every text-sized box is then measured against a ruler, not a font.
    pub text_metrics_font_derived: bool,
    /// Why a page field above is null.
    pub unavailable: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Identity {
    pub node_id: usize,
    pub tag: String,
    pub id: Option<String>,
    pub classes: Vec<String>,
    /// Unique structural selector, `html > body > div:nth-child(2) > p`.
    pub selector: String,
    /// The parity oracle's join key (`getSelector` in capture_baseline.mjs).
    pub oracle_key: String,
    /// `tag#id.class` entries, nearest ancestor first, up to `html`.
    pub ancestry: Vec<String>,
    /// Index among the parent's element children; `None` for the root.
    pub child_index: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TreeState {
    pub in_dom: bool,
    pub connected: bool,
    pub in_shadow_tree: Option<bool>,
    pub shadow_host: Option<String>,
    pub assigned_slot: Option<String>,
    /// Element children (`children.length`).
    pub child_count: usize,
    /// `textContent.length`, in UTF-16 units. Text itself is never reported.
    pub text_length: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ContainingBlock {
    /// Selector of the containing block's element; `None` = the viewport.
    pub selector: Option<String>,
    pub content_box: DiagRect,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct BoxInfo {
    pub has_box: bool,
    pub no_box_reason: Option<String>,
    /// Viewport coordinates, as `getBoundingClientRect`.
    pub border_box: Option<DiagRect>,
    pub padding_box: Option<DiagRect>,
    pub content_box: Option<DiagRect>,
    pub containing_block: Option<ContainingBlock>,
    /// `block`, `inline`, `float`, `flex item of <sel>`, `grid item of <sel>`,
    /// `table cell`, `absolute relative to <sel|viewport>`, `fixed`.
    pub formatting_context: Option<String>,
    /// Placed `[column-start, column-end, row-start, row-end]` of a grid item.
    pub grid_lines: Option<[i32; 4]>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct PaintInfo {
    pub display_commands: Option<usize>,
    pub command_kinds: Option<Vec<String>>,
    pub clipped: Option<bool>,
    pub clipped_by: Option<String>,
    pub outside_viewport: Option<bool>,
    pub effective_opacity: Option<f32>,
    pub in_opacity_layer: Option<bool>,
    pub in_transform_layer: Option<bool>,
}

/// One element's record. Serialises to well under 4 KB of JSON.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ElementDiagnostics {
    pub schema: &'static str,
    /// Hoisted to the dump's top level in `all_element_diagnostics`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<PageInfo>,
    pub identity: Identity,
    pub tree: TreeState,
    pub style: BTreeMap<&'static str, Option<String>>,
    #[serde(rename = "box")]
    pub layout_box: BoxInfo,
    pub paint: PaintInfo,
    pub notes: Vec<String>,
    pub unavailable: BTreeMap<String, String>,
}

/// Every element of a page (`Engine::all_element_diagnostics`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DiagnosticsDump {
    pub schema: &'static str,
    pub page: PageInfo,
    pub total_elements: usize,
    pub truncated: bool,
    pub elements: Vec<ElementDiagnostics>,
}

// ---------------------------------------------------------------------------
// The layout index: built on demand from the committed tree.
// ---------------------------------------------------------------------------

/// What a box's ancestors decide about it.
struct BoxContext<'a> {
    boxes: Vec<&'a LayoutBox>,
    /// The nearest layout ancestor (possibly anonymous).
    parent: Option<&'a LayoutBox>,
    /// `None` = the initial containing block.
    containing_block: Option<&'a LayoutBox>,
    clipped_by: Option<&'a LayoutBox>,
    opacity: f32,
    opacity_layer: bool,
    transform_layer: bool,
}

#[derive(Clone, Copy)]
struct Inherited<'a> {
    parent: Option<&'a LayoutBox>,
    block_cb: Option<&'a LayoutBox>,
    positioned_cb: Option<&'a LayoutBox>,
    opacity: f32,
    opacity_layer: bool,
    transform_layer: bool,
}

/// Clip ancestors, nearest first, as a stack-allocated list.
struct ClipLink<'a, 'b> {
    clip: &'a LayoutBox,
    up: Option<&'b ClipLink<'a, 'b>>,
}

fn clips(b: &LayoutBox) -> bool {
    b.style.overflow_x != Overflow::Visible || b.style.overflow_y != Overflow::Visible
}

fn is_block_container(b: &LayoutBox) -> bool {
    !matches!(b.box_type, BoxType::Inline | BoxType::Text(_) | BoxType::LineBreak)
}

fn contains(outer: &Rect, inner: &Rect) -> bool {
    const EPS: f32 = 0.01;
    inner.x >= outer.x - EPS
        && inner.y >= outer.y - EPS
        && inner.x + inner.width <= outer.x + outer.width + EPS
        && inner.y + inner.height <= outer.y + outer.height + EPS
}

fn index_layout<'a>(root: &'a LayoutBox) -> HashMap<usize, BoxContext<'a>> {
    fn walk<'a>(
        b: &'a LayoutBox,
        inh: Inherited<'a>,
        clip: Option<&ClipLink<'a, '_>>,
        out: &mut HashMap<usize, BoxContext<'a>>,
    ) {
        let s = &b.style;
        let opacity = inh.opacity * s.opacity;
        let transformed = !s.effective_transform().is_identity();
        let opacity_layer = inh.opacity_layer || s.opacity < 1.0;
        let transform_layer = inh.transform_layer || transformed;
        if let Some(node) = b.node_id {
            match out.get_mut(&node) {
                Some(ctx) => ctx.boxes.push(b),
                None => {
                    let containing_block = match s.position {
                        rustkit_css::Position::Fixed => None,
                        rustkit_css::Position::Absolute => inh.positioned_cb,
                        _ => inh.block_cb,
                    };
                    let bb = b.dimensions.border_box();
                    let mut clipped_by = None;
                    let mut link = clip;
                    while let Some(l) = link {
                        if !contains(&l.clip.dimensions.padding_box(), &bb) {
                            clipped_by = Some(l.clip);
                            break;
                        }
                        link = l.up;
                    }
                    out.insert(
                        node,
                        BoxContext {
                            boxes: vec![b],
                            parent: inh.parent,
                            containing_block,
                            clipped_by,
                            opacity,
                            opacity_layer,
                            transform_layer,
                        },
                    );
                }
            }
        }
        let element = b.node_id.is_some();
        let next = Inherited {
            parent: Some(b),
            block_cb: if element && is_block_container(b) { Some(b) } else { inh.block_cb },
            positioned_cb: if element && (s.position != rustkit_css::Position::Static || transformed) {
                Some(b)
            } else {
                inh.positioned_cb
            },
            opacity,
            opacity_layer,
            transform_layer,
        };
        let own = ClipLink { clip: b, up: clip };
        let child_clip = if element && clips(b) { Some(&own) } else { clip };
        for child in &b.children {
            walk(child, next, child_clip, out);
        }
    }
    let mut out = HashMap::new();
    let inh = Inherited {
        parent: None,
        block_cb: None,
        positioned_cb: None,
        opacity: 1.0,
        opacity_layer: false,
        transform_layer: false,
    };
    walk(root, inh, None, &mut out);
    out
}

// ---------------------------------------------------------------------------
// Building records
// ---------------------------------------------------------------------------

/// Borrowed state one record is built from.
pub(crate) struct DiagnosticsSource<'a> {
    pub document: &'a Document,
    pub layout: Option<&'a LayoutBox>,
    pub scroll: (f32, f32),
    pub viewport: (u32, u32),
    pub checked: &'a HashMap<usize, bool>,
}

struct Builder<'a> {
    src: &'a DiagnosticsSource<'a>,
    index: HashMap<usize, BoxContext<'a>>,
    text_len: HashMap<usize, usize>,
    /// The `html` element, when its geometry is the anonymous layout root's.
    root_stand_in: Option<usize>,
}

fn element_parts(node: &Node) -> Option<(String, &HashMap<String, String>)> {
    match &node.node_type {
        NodeType::Element { tag_name, attributes, .. } => Some((tag_name.to_lowercase(), attributes)),
        _ => None,
    }
}

fn element_children(node: &Node) -> Vec<Rc<Node>> {
    node.children().into_iter().filter(|c| matches!(c.node_type, NodeType::Element { .. })).collect()
}

fn parent_element(node: &Node) -> Option<Rc<Node>> {
    node.parent().filter(|p| matches!(p.node_type, NodeType::Element { .. }))
}

fn classes_of(attrs: &HashMap<String, String>) -> Vec<String> {
    attrs.get("class").map(|c| c.split_whitespace().map(str::to_string).collect()).unwrap_or_default()
}

/// `tag#id.class.class`, as DevTools' breadcrumb writes it.
fn short_label(node: &Node) -> String {
    let Some((tag, attrs)) = element_parts(node) else { return String::new() };
    let mut out = tag;
    if let Some(id) = attrs.get("id").filter(|i| !i.is_empty()) {
        out.push('#');
        out.push_str(id);
    }
    for c in classes_of(attrs) {
        out.push('.');
        out.push_str(&c);
    }
    out
}

/// Unique structural selector: tag and `:nth-child(n)` when the element has
/// element siblings, from the root element down.
fn structural_selector(node: &Rc<Node>) -> String {
    let mut parts = Vec::new();
    let mut cur = Some(node.clone());
    while let Some(n) = cur {
        let Some((tag, _)) = element_parts(&n) else { break };
        let parent = parent_element(&n);
        let seg = match n.parent() {
            Some(p) => {
                let kids = element_children(&p);
                match kids.iter().position(|k| k.id == n.id) {
                    Some(i) if kids.len() > 1 => format!("{tag}:nth-child({})", i + 1),
                    _ => tag,
                }
            }
            None => tag,
        };
        parts.push(seg);
        cur = parent;
    }
    parts.reverse();
    parts.join(" > ")
}

/// The oracle's `getSelector` key, through the same segment builder the
/// layout identity uses, for an element that has no box to carry one.
fn oracle_key(node: &Rc<Node>) -> String {
    let mut parts = Vec::new();
    let mut cur = Some(node.clone());
    let mut foreign = false;
    // Foreign content is decided top-down; find whether any ancestor is svg/math.
    let mut a = Some(node.clone());
    while let Some(n) = a {
        if let Some((tag, _)) = element_parts(&n) {
            if Engine::enters_foreign_content(&tag) {
                foreign = true;
            }
        }
        a = parent_element(&n);
    }
    while let Some(n) = cur {
        let Some((tag, attrs)) = element_parts(&n) else { break };
        let (index, total) = match n.parent() {
            Some(p) => {
                let same: Vec<_> = element_children(&p)
                    .into_iter()
                    .filter(|k| element_parts(k).is_some_and(|(t, _)| t == tag))
                    .collect();
                (same.iter().position(|k| k.id == n.id).map_or(1, |i| i + 1), same.len())
            }
            None => (1, 1),
        };
        parts.push(Engine::selector_segment(&tag, attrs, index, total, foreign));
        if Engine::enters_foreign_content(&tag) {
            foreign = false;
        }
        let parent = parent_element(&n);
        if parent.as_ref().and_then(|p| element_parts(p)).is_some_and(|(t, _)| t == "body") {
            parts.push("body".into());
            break;
        }
        cur = parent;
    }
    parts.reverse();
    let path = parts.join(" > ");
    match element_parts(node) {
        Some((_, attrs)) => Engine::reported_selector(&path, attrs),
        None => path,
    }
}

fn text_lengths(root: &Rc<Node>, out: &mut HashMap<usize, usize>) -> usize {
    let len = match &root.node_type {
        NodeType::Text(t) => t.encode_utf16().count(),
        NodeType::Comment(_) | NodeType::ProcessingInstruction { .. } => 0,
        _ => root.children().iter().map(|c| text_lengths(c, out)).sum(),
    };
    if matches!(root.node_type, NodeType::Element { .. }) {
        out.insert(root.id.raw(), len);
    }
    len
}

fn to_viewport(r: Rect, scroll: (f32, f32)) -> DiagRect {
    let round = |v: f32| (v * 100.0).round() / 100.0 + 0.0;
    DiagRect { x: round(r.x - scroll.0), y: round(r.y - scroll.1), w: round(r.width), h: round(r.height) }
}

fn union(a: Rect, b: Rect) -> Rect {
    let (l, t) = (a.x.min(b.x), a.y.min(b.y));
    let (r, bt) = ((a.x + a.width).max(b.x + b.width), (a.y + a.height).max(b.y + b.height));
    Rect { x: l, y: t, width: r - l, height: bt - t }
}

/// The page URL as a record reports it: no query or fragment unless asked.
pub(crate) fn report_url(url: &Url, include_query: bool) -> String {
    if include_query {
        return url.to_string();
    }
    let mut u = url.clone();
    u.set_query(None);
    u.set_fragment(None);
    u.to_string()
}

impl<'a> Builder<'a> {
    fn new(src: &'a DiagnosticsSource<'a>) -> Self {
        let mut index = src.layout.map(index_layout).unwrap_or_default();
        // The layout root is an anonymous stand-in for `html`: its geometry
        // is html's, its style is the default (html's own only feeds
        // inheritance), so html is given the root's box but no style.
        let mut root_stand_in = None;
        if let (Some(root), Some(html)) = (src.layout.filter(|r| r.node_id.is_none()), src.document.document_element()) {
            let raw = html.id.raw();
            if let std::collections::hash_map::Entry::Vacant(e) = index.entry(raw) {
                e.insert(BoxContext {
                    boxes: vec![root],
                    parent: None,
                    containing_block: None,
                    clipped_by: None,
                    opacity: 1.0,
                    opacity_layer: false,
                    transform_layer: false,
                });
                root_stand_in = Some(raw);
            }
        }
        let mut text_len = HashMap::new();
        text_lengths(src.document.root(), &mut text_len);
        Self { src, index, text_len, root_stand_in }
    }

    fn selector_of_node(&self, raw: usize) -> Option<String> {
        self.src.document.get_node(rustkit_dom::NodeId::new(raw)).map(|n| structural_selector(&n))
    }

    fn record(&self, node: &Rc<Node>) -> Option<ElementDiagnostics> {
        let (tag, attrs) = element_parts(node)?;
        let raw = node.id.raw();
        let mut unavailable = BTreeMap::new();
        let mut notes = Vec::new();
        let ctx = self.index.get(&raw);

        let mut connected = false;
        let mut root = node.clone();
        while let Some(p) = root.parent() {
            root = p;
        }
        if matches!(root.node_type, NodeType::Document) && Rc::ptr_eq(&root, self.src.document.root()) {
            connected = true;
        }
        let parent = parent_element(node);
        let identity = Identity {
            node_id: raw,
            tag: tag.clone(),
            id: attrs.get("id").filter(|i| !i.is_empty()).cloned(),
            classes: classes_of(attrs),
            selector: structural_selector(node),
            oracle_key: ctx
                .and_then(|c| c.boxes[0].identity().map(|i| i.selector.clone()))
                .unwrap_or_else(|| oracle_key(node)),
            ancestry: {
                let mut v = Vec::new();
                let mut cur = parent.clone();
                while let Some(p) = cur {
                    v.push(short_label(&p));
                    cur = parent_element(&p);
                }
                v
            },
            child_index: node.parent().and_then(|p| element_children(&p).iter().position(|k| k.id == node.id)),
        };

        const SHADOW: &str = "shadow trees and slots live in script only (web_shadow.js)";
        let tree = TreeState {
            in_dom: true,
            connected,
            in_shadow_tree: if connected { Some(false) } else { None },
            shadow_host: None,
            assigned_slot: None,
            child_count: element_children(node).len(),
            text_length: self.text_len.get(&raw).copied().unwrap_or_else(|| text_lengths(node, &mut HashMap::new())),
        };
        if !connected {
            unavailable.insert("tree.in_shadow_tree".into(), SHADOW.into());
        }
        unavailable.insert("tree.shadow_host".into(), SHADOW.into());
        unavailable.insert("tree.assigned_slot".into(), SHADOW.into());

        const NO_ATTRIBUTION: &str = "display-list commands carry no node id";
        unavailable.insert("paint.display_commands".into(), NO_ATTRIBUTION.into());
        unavailable.insert("paint.command_kinds".into(), NO_ATTRIBUTION.into());

        let (style, layout_box, paint) = match ctx {
            Some(ctx) => {
                let (mut style, layout_box, paint) = self.boxed(ctx, &mut notes, &mut unavailable);
                if tag == "body" && self.body_background_moved(ctx.boxes[0]) {
                    for p in ["background-color", "background-image"] {
                        style.insert(p, None);
                        unavailable.insert(
                            format!("style.{p}"),
                            "moved to the canvas (CSS 2.1 §14.2); the body box keeps none".into(),
                        );
                    }
                }
                if self.root_stand_in == Some(raw) {
                    style.values_mut().for_each(|v| *v = None);
                    unavailable.retain(|k, _| !k.starts_with("style."));
                    unavailable.insert("style".into(), "html is laid out by an anonymous root box with the default style".into());
                }
                (style, layout_box, paint)
            }
            None => {
                let reason = self.no_box_reason(node, &tag, connected);
                let mut style: BTreeMap<&'static str, Option<String>> =
                    STYLE_PROPERTIES.iter().map(|p| (*p, None)).collect();
                // An element with no box under a boxed parent is display:none
                // (as rustkit-bindings' computedMap answers); deeper ones keep
                // their own display, which the engine never computed.
                if reason.starts_with("display:none") || reason.starts_with("not rendered") {
                    style.insert("display", Some("none".into()));
                }
                unavailable.insert("style".into(), "no layout box: the engine keeps computed style only on boxes".into());
                for f in ["box", "paint"] {
                    unavailable.insert(f.into(), "no layout box".into());
                }
                (style, BoxInfo { no_box_reason: Some(reason), ..Default::default() }, PaintInfo::default())
            }
        };

        Some(ElementDiagnostics {
            schema: DIAGNOSTICS_SCHEMA,
            page: None,
            identity,
            tree,
            style,
            layout_box,
            paint,
            notes,
            unavailable,
        })
    }

    /// The engine transfers body's background to the root box when html's
    /// is transparent and clears it from body, so body's own is not kept.
    fn body_background_moved(&self, body: &LayoutBox) -> bool {
        let has = |s: &rustkit_css::ComputedStyle| {
            s.background_color.a > 0.0 || s.background_gradient.is_some() || !s.background_layers.is_empty()
        };
        self.src.layout.is_some_and(|root| root.node_id.is_none() && has(&root.style)) && !has(&body.style)
    }

    fn no_box_reason(&self, node: &Rc<Node>, tag: &str, connected: bool) -> String {
        if !connected {
            return "not connected to the document".into();
        }
        if self.src.layout.is_none() {
            return "the view has no layout yet".into();
        }
        if matches!(tag, "html") {
            return "the layout root is an anonymous stand-in for html; html's own box is not built".into();
        }
        if matches!(tag, "head" | "script" | "style" | "meta" | "link" | "title" | "template" | "noscript" | "base") {
            return format!("not rendered: {tag} is display:none in the UA sheet");
        }
        let parent = parent_element(node);
        let mut cur = parent.clone();
        let mut svg = false;
        while let Some(p) = cur {
            svg |= element_parts(&p).is_some_and(|(t, _)| t == "svg");
            if self.index.contains_key(&p.id.raw()) {
                return if svg {
                    "SVG content: painted by the <svg> box, which builds no CSS box for it".into()
                } else if parent.as_ref().is_some_and(|q| q.id == p.id) {
                    "display:none, or dropped by the box tree builder".into()
                } else {
                    format!("an ancestor has no box (nearest boxed ancestor: {})", structural_selector(&p))
                };
            }
            cur = parent_element(&p);
        }
        "no ancestor has a box".into()
    }

    fn boxed(
        &self,
        ctx: &BoxContext<'a>,
        notes: &mut Vec<String>,
        unavailable: &mut BTreeMap<String, String>,
    ) -> (BTreeMap<&'static str, Option<String>>, BoxInfo, PaintInfo) {
        let b = ctx.boxes[0];
        let scroll = self.src.scroll;
        let (mut border, mut padding, mut content) =
            (b.dimensions.border_box(), b.dimensions.padding_box(), b.dimensions.content);
        for other in &ctx.boxes[1..] {
            border = union(border, other.dimensions.border_box());
            padding = union(padding, other.dimensions.padding_box());
            content = union(content, other.dimensions.content);
        }
        if ctx.boxes.len() > 1 {
            notes.push(format!("laid out as {} boxes; rects are their union", ctx.boxes.len()));
        }
        let style = style_record(b, notes, unavailable);
        let position = b.style.position;
        if position == rustkit_css::Position::Fixed && scroll != (0.0, 0.0) {
            notes.push("fixed box: viewport rect is layout minus scroll, as getBoundingClientRect here".into());
        }

        let selector_of = |l: &LayoutBox| l.node_id.and_then(|n| self.selector_of_node(n));
        let containing_block = Some(match ctx.containing_block {
            Some(cb) => ContainingBlock { selector: selector_of(cb), content_box: to_viewport(cb.dimensions.content, scroll) },
            None => {
                let (w, h) = self.src.viewport;
                ContainingBlock { selector: None, content_box: DiagRect { x: 0.0, y: 0.0, w: w as f32, h: h as f32 } }
            }
        });
        let parent_display = ctx.parent.map(|p| p.style.display);
        let parent_sel = || ctx.parent.and_then(selector_of).unwrap_or_else(|| "anonymous".into());
        let mut grid_item = false;
        let fc = match position {
            rustkit_css::Position::Fixed => "fixed".to_string(),
            rustkit_css::Position::Absolute => format!(
                "absolute relative to {}",
                ctx.containing_block.and_then(selector_of).unwrap_or_else(|| "viewport".into())
            ),
            _ if b.style.float != rustkit_css::Float::None => "float".into(),
            _ if b.style.display == Display::TableCell => "table cell".into(),
            _ if parent_display.is_some_and(|d| d.is_flex()) => format!("flex item of {}", parent_sel()),
            _ if parent_display.is_some_and(|d| matches!(d, Display::Grid | Display::InlineGrid)) => {
                grid_item = true;
                format!("grid item of {}", parent_sel())
            }
            _ if matches!(b.box_type, BoxType::Inline | BoxType::Text(_))
                || matches!(
                    b.style.display,
                    Display::Inline | Display::InlineBlock | Display::InlineFlex | Display::InlineGrid | Display::InlineTable
                ) =>
            {
                "inline".into()
            }
            _ => "block".into(),
        };
        if grid_item {
            unavailable.insert("box.grid_lines".into(), "grid placement is not kept on the box".into());
        }
        if position == rustkit_css::Position::Absolute && ctx.clipped_by.is_some() {
            notes.push("clip ancestry does not model an abspos box escaping a non-positioned clip".into());
        }

        let viewport_border = to_viewport(border, scroll);
        let (vw, vh) = (self.src.viewport.0 as f32, self.src.viewport.1 as f32);
        let outside = viewport_border.x + viewport_border.w <= 0.0
            || viewport_border.y + viewport_border.h <= 0.0
            || viewport_border.x >= vw
            || viewport_border.y >= vh;
        let layout_box = BoxInfo {
            has_box: true,
            border_box: Some(viewport_border),
            padding_box: Some(to_viewport(padding, scroll)),
            content_box: Some(to_viewport(content, scroll)),
            containing_block,
            formatting_context: Some(fc),
            ..Default::default()
        };
        let paint = PaintInfo {
            clipped: Some(ctx.clipped_by.is_some()),
            clipped_by: ctx.clipped_by.and_then(selector_of),
            outside_viewport: Some(outside),
            effective_opacity: Some((ctx.opacity * 1000.0).round() / 1000.0),
            in_opacity_layer: Some(ctx.opacity_layer),
            in_transform_layer: Some(ctx.transform_layer),
            ..Default::default()
        };
        (style, layout_box, paint)
    }
}

// ---------------------------------------------------------------------------
// Computed values, serialised as getComputedStyle does
// ---------------------------------------------------------------------------

fn num(v: f32) -> String {
    let r = (v * 1000.0).round() / 1000.0 + 0.0;
    if r.fract() == 0.0 { format!("{}", r as i64) } else { format!("{r}") }
}

/// A specified length that has not been resolved against a box: absolute
/// and font/viewport-relative lengths in px, percentages as written.
fn length(l: &Length, none_for_auto: bool, font_px: f32, viewport: (f32, f32)) -> String {
    match l {
        Length::Auto if none_for_auto => "none".into(),
        Length::Auto => "auto".into(),
        Length::Zero => "0px".into(),
        Length::Px(p) => css_px(*p),
        Length::Percent(p) => format!("{}%", num(*p)),
        Length::FitContent => "fit-content".into(),
        other => css_px(other.to_px_with_viewport(font_px, 16.0, 0.0, viewport.0, viewport.1)),
    }
}

fn track(t: &TrackSize) -> String {
    match t {
        TrackSize::Px(p) => css_px(*p),
        TrackSize::Percent(p) => format!("{}%", num(*p)),
        TrackSize::Fr(f) => format!("{}fr", num(*f)),
        TrackSize::MinContent => "min-content".into(),
        TrackSize::MaxContent => "max-content".into(),
        TrackSize::Auto => "auto".into(),
        TrackSize::MinMax(a, b) => format!("minmax({}, {})", track(a), track(b)),
        TrackSize::FitContent(p) => format!("fit-content({})", css_px(*p)),
    }
}

fn template(t: &rustkit_css::GridTemplate) -> String {
    if t.tracks.is_empty() && t.repeats.is_empty() {
        return "none".into();
    }
    let mut parts: Vec<String> = t.tracks.iter().map(|d| track(&d.size)).collect();
    for (at, rep) in t.repeats.iter().rev() {
        let (count, defs) = match rep {
            rustkit_css::TrackRepeat::Count(n, d) => (n.to_string(), d),
            rustkit_css::TrackRepeat::AutoFill(d) => ("auto-fill".to_string(), d),
            rustkit_css::TrackRepeat::AutoFit(d) => ("auto-fit".to_string(), d),
        };
        let inner: Vec<String> = defs.iter().map(|d| track(&d.size)).collect();
        parts.insert((*at).min(parts.len()), format!("repeat({count}, {})", inner.join(" ")));
    }
    parts.join(" ")
}

fn grid_line(l: &GridLine) -> String {
    match l {
        GridLine::Auto => "auto".into(),
        GridLine::Number(n) => n.to_string(),
        GridLine::Name(s) => s.clone(),
        GridLine::Span(n) => format!("span {n}"),
        GridLine::SpanName(s) => format!("span {s}"),
    }
}

/// `margin`-style shorthand collapsing of four sides.
fn four(t: String, r: String, b: String, l: String) -> String {
    if r == l {
        if t == b {
            if t == r { t } else { format!("{t} {r}") }
        } else {
            format!("{t} {r} {b}")
        }
    } else {
        format!("{t} {r} {b} {l}")
    }
}

/// `url("…")` without the query, `data:` contents elided, gradients named
/// but not spelled out. tools/inspector normalises Chromium's value the same way.
pub(crate) fn background_image(b: &LayoutBox) -> String {
    let mut out = Vec::new();
    for layer in &b.style.background_layers {
        out.push(match &layer.image {
            rustkit_css::BackgroundImage::None => "none".to_string(),
            rustkit_css::BackgroundImage::Url(u) => format!("url(\"{}\")", scrub_url(u)),
            rustkit_css::BackgroundImage::Gradient(g) => gradient_name(&format!("{g:?}")),
        });
    }
    if out.is_empty() || out.iter().all(|v| v == "none") {
        if let Some(g) = &b.style.background_gradient {
            return gradient_name(&format!("{g:?}"));
        }
        return "none".into();
    }
    out.join(", ")
}

fn gradient_name(debug: &str) -> String {
    let kind = debug.split(|c: char| !c.is_alphanumeric()).next().unwrap_or("");
    let repeating = debug.contains("repeating: true");
    let base = match kind {
        k if k.starts_with("Radial") => "radial-gradient",
        k if k.starts_with("Conic") => "conic-gradient",
        _ => "linear-gradient",
    };
    format!("{}{base}(…)", if repeating { "repeating-" } else { "" })
}

/// A URL inside a record: no query or fragment, `data:` contents elided.
pub(crate) fn scrub_url(u: &str) -> String {
    if u.starts_with("data:") {
        return "data:…".into();
    }
    u.split(['?', '#']).next().unwrap_or("").to_string()
}

fn style_record(
    b: &LayoutBox,
    notes: &mut Vec<String>,
    unavailable: &mut BTreeMap<String, String>,
) -> BTreeMap<&'static str, Option<String>> {
    let s = &*b.style;
    let d = &b.dimensions;
    // The lines script reads (display, position, used sizes and edges, ...).
    let mut m: BTreeMap<&'static str, Option<String>> = BTreeMap::new();
    let base = computed_style_lines(b);
    let base: HashMap<&str, &str> = base.lines().filter_map(|l| l.split_once('\t')).collect();
    let kw = |v: &dyn std::fmt::Debug| css_keyword(&format!("{v:?}"));
    let inline_non_replaced = matches!(b.box_type, BoxType::Inline);
    let font_px = match s.font_size {
        Length::Px(p) => p,
        _ => 16.0,
    };
    let len = |l: &Length, none_for_auto| length(l, none_for_auto, font_px, b.viewport);
    // getComputedStyle's width/height follow box-sizing.
    let border_box_sizing = s.box_sizing == rustkit_css::BoxSizing::BorderBox;
    for p in STYLE_PROPERTIES {
        let v = match *p {
            "width" | "height" if inline_non_replaced => Some("auto".into()),
            "width" if border_box_sizing => Some(css_px(d.border_box().width)),
            "height" if border_box_sizing => Some(css_px(d.border_box().height)),
            "clear" => Some(kw(&s.clear)),
            "min-width" => Some(len(&s.min_width, false)),
            "min-height" => Some(len(&s.min_height, false)),
            "max-width" => Some(len(&s.max_width, true)),
            "max-height" => Some(len(&s.max_height, true)),
            "inset" => Some(four(
                base["top"].to_string(),
                base["right"].to_string(),
                base["bottom"].to_string(),
                base["left"].to_string(),
            )),
            "opacity" => Some(num(s.opacity)),
            "transform" => Some({
                let t = s.effective_transform();
                if t.is_identity() {
                    "none".into()
                } else {
                    let bb = d.border_box();
                    let m6 = t.to_matrix(bb.width, bb.height);
                    format!("matrix({})", m6.iter().map(|v| num(*v)).collect::<Vec<_>>().join(", "))
                }
            }),
            "flex-wrap" => Some(kw(&s.flex_wrap)),
            "flex-grow" => Some(num(s.flex_grow)),
            "flex-shrink" => Some(num(s.flex_shrink)),
            "flex-basis" => Some(match s.flex_basis {
                rustkit_css::FlexBasis::Auto => "auto".into(),
                rustkit_css::FlexBasis::Content => "content".into(),
                rustkit_css::FlexBasis::Length(px) => css_px(px),
                rustkit_css::FlexBasis::Percent(pc) => format!("{}%", num(pc)),
            }),
            "align-items" => Some(kw(&s.align_items)),
            "align-self" => Some(kw(&s.align_self)),
            "justify-content" => Some(kw(&s.justify_content)),
            "justify-self" => Some(kw(&s.justify_self)),
            "order" => Some(s.order.to_string()),
            "row-gap" => Some(len(&s.row_gap, false)),
            "column-gap" => Some(len(&s.column_gap, false)),
            "gap" => {
                let (r, c) = (len(&s.row_gap, false), len(&s.column_gap, false));
                Some(if r == c { r } else { format!("{r} {c}") })
            }
            "grid-template-columns" => Some(template(&s.grid_template_columns)),
            "grid-template-rows" => Some(template(&s.grid_template_rows)),
            "grid-auto-flow" => Some(match s.grid_auto_flow {
                rustkit_css::GridAutoFlow::Row => "row".into(),
                rustkit_css::GridAutoFlow::Column => "column".into(),
                rustkit_css::GridAutoFlow::RowDense => "row dense".into(),
                rustkit_css::GridAutoFlow::ColumnDense => "column dense".into(),
            }),
            "grid-auto-rows" => Some(track(&s.grid_auto_rows)),
            "grid-auto-columns" => Some(track(&s.grid_auto_columns)),
            "grid-column-start" => Some(grid_line(&s.grid_column_start)),
            "grid-column-end" => Some(grid_line(&s.grid_column_end)),
            "grid-row-start" => Some(grid_line(&s.grid_row_start)),
            "grid-row-end" => Some(grid_line(&s.grid_row_end)),
            "aspect-ratio" => Some(s.aspect_ratio.map_or("auto".into(), |r| format!("{} / 1", num(r)))),
            "vertical-align" => Some(match s.vertical_align {
                rustkit_css::VerticalAlign::Length(px) => css_px(px),
                v => kw(&v),
            }),
            "background-image" => Some(background_image(b)),
            "object-fit" => Some(s.object_fit.clone()),
            "content-visibility" | "contain" => {
                unavailable.insert(format!("style.{p}"), "not modelled by the engine".into());
                None
            }
            other => base.get(other).map(|v| v.to_string()),
        };
        m.insert(p, v);
    }
    if matches!(s.display, Display::Grid | Display::InlineGrid) {
        notes.push("grid-template-* is the specified track list, not the resolved px getComputedStyle reports".into());
    }
    m
}

impl Engine {
    fn diagnostics_page(&self, view: &ViewState, opts: &DiagnosticsOptions) -> PageInfo {
        let mut unavailable = BTreeMap::new();
        let viewport = match view.headless_bounds {
            Some(b) => (b.width, b.height),
            None => self.compositor.get_surface_size(view.viewhost_id).unwrap_or((0, 0)),
        };
        let commit = option_env!("HIWAVE_BUILD_COMMIT").map(str::to_string);
        if commit.is_none() {
            unavailable.insert("engine.commit".into(), "HIWAVE_BUILD_COMMIT was not set at compile time".into());
        }
        let device_scale = view.headless_bounds.map(|_| 1.0);
        if device_scale.is_none() {
            unavailable.insert("device_scale".into(), "the engine lays out in CSS px and does not track the window's backing scale".into());
        }
        PageInfo {
            url: view.url.as_ref().map(|u| report_url(u, opts.include_url_query)),
            engine: EngineBuild { version: env!("CARGO_PKG_VERSION").to_string(), commit },
            viewport,
            device_scale,
            scroll: view.scroll_offset,
            text_metrics_font_derived: rustkit_layout::TEXT_METRICS_ARE_FONT_DERIVED,
            unavailable,
        }
    }

    fn diagnostics_source<'a>(&self, view: &'a ViewState, page: &PageInfo) -> Result<DiagnosticsSource<'a>, EngineError> {
        let document = view
            .document
            .as_deref()
            .ok_or_else(|| EngineError::RenderError("No document available".into()))?;
        Ok(DiagnosticsSource {
            document,
            layout: view.layout.as_ref(),
            scroll: view.scroll_offset,
            viewport: page.viewport,
            checked: &view.checked_states,
        })
    }

    /// The diagnostics record of one element, or `None` when no element
    /// matches the target.
    ///
    /// Read-only over the view's LAST COMMITTED layout tree and DOM: it takes
    /// `&self`, runs no script, triggers no layout, and borrows the DOM's
    /// `RefCell`s (children, node map) only for the duration of the call. An
    /// app can therefore call it from the window thread between turns, and it
    /// answers from the last completed layout even while the page has script
    /// work pending. A selector is matched with the cascade's matcher; an
    /// invalid one is an error.
    pub fn element_diagnostics(
        &self,
        id: EngineViewId,
        target: &DiagnosticTarget,
        opts: &DiagnosticsOptions,
    ) -> Result<Option<ElementDiagnostics>, EngineError> {
        let view = self.views.get(&id).ok_or(EngineError::ViewNotFound(id))?;
        let page = self.diagnostics_page(view, opts);
        let src = self.diagnostics_source(view, &page)?;
        let node = match target {
            DiagnosticTarget::Node(raw) => src.document.get_node(rustkit_dom::NodeId::new(*raw)),
            DiagnosticTarget::Point { x, y } => src
                .layout
                .and_then(|l| l.hit_test(x + src.scroll.0, y + src.scroll.1))
                .and_then(|hit| hit.node_id)
                .and_then(|raw| src.document.get_node(rustkit_dom::NodeId::new(raw))),
            DiagnosticTarget::Selector(sel) => {
                let mut stack = vec![src.document.root().clone()];
                let mut found = None;
                while let Some(n) = stack.pop() {
                    match SelectorMatcher.node_matches(&n, sel, src.checked) {
                        None => return Err(EngineError::RenderError(format!("invalid selector: {sel}"))),
                        Some(true) => {
                            found = Some(n);
                            break;
                        }
                        Some(false) => {}
                    }
                    stack.extend(n.children().into_iter().rev());
                }
                found
            }
        };
        let Some(node) = node else { return Ok(None) };
        let builder = Builder::new(&src);
        Ok(builder.record(&node).map(|mut r| {
            r.page = Some(page);
            r
        }))
    }

    /// A record for every element of the document in tree order, with or
    /// without a box, up to `opts.max_elements` (`truncated` says when the
    /// cap cut the list). Same borrowing as [`Engine::element_diagnostics`].
    pub fn all_element_diagnostics(
        &self,
        id: EngineViewId,
        opts: &DiagnosticsOptions,
    ) -> Result<DiagnosticsDump, EngineError> {
        let view = self.views.get(&id).ok_or(EngineError::ViewNotFound(id))?;
        let page = self.diagnostics_page(view, opts);
        let src = self.diagnostics_source(view, &page)?;
        let builder = Builder::new(&src);
        let mut elements = Vec::new();
        let mut total = 0;
        let mut stack = vec![src.document.root().clone()];
        while let Some(n) = stack.pop() {
            if matches!(n.node_type, NodeType::Element { .. }) {
                total += 1;
                if elements.len() < opts.max_elements {
                    elements.extend(builder.record(&n));
                }
            }
            stack.extend(n.children().into_iter().rev());
        }
        Ok(DiagnosticsDump {
            schema: DIAGNOSTICS_SCHEMA,
            page,
            total_elements: total,
            truncated: total > elements.len(),
            elements,
        })
    }
}
