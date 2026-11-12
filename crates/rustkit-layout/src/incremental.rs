//! Incremental layout, stage 1 (`docs/design/INCREMENTAL_LAYOUT_PLAN.md`).
//!
//! Given the box tree the last layout started from (unlaid), the tree it
//! produced (laid out), and a freshly built unlaid tree, find the one block
//! box whose content changed, lay out only that box where the full pass put
//! it, and move what follows it by its height delta, growing every ancestor
//! up to the root.
//!
//! Only the simplest case is taken: see [`relayout`] for the conditions.
//! Everything else returns a [`Fallback`] reason and the caller lays the
//! whole tree out, which is always correct. The engine's verify mode runs
//! both and compares every box rect ([`compare_rects`]).

use crate::{
    BoxType, Dimensions, EdgeSizes, FloatContext, LayoutBox, MarginCollapseContext, Position, Rect,
};
use rustkit_css::{Clear, ContainerType, Display, Float, Length, Overflow, WritingMode};
use std::cell::Cell;

thread_local! {
    static VISITS: Cell<u64> = const { Cell::new(0) };
}

/// Count one box laid out (the layout entry points call this).
pub(crate) fn count_visit() {
    VISITS.with(|v| v.set(v.get().wrapping_add(1)));
}

/// How many boxes the layout entry points have laid out on this thread, in
/// all. Tests and the engine read the difference across a layout.
pub fn layout_visits() -> u64 {
    VISITS.with(Cell::get)
}

/// Why the incremental path did not apply. The full layout is the answer.
pub type Fallback = &'static str;

/// What an incremental relayout did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Relaid {
    /// Child-index path from the root to the box laid out again; `None`
    /// when nothing changed and the old layout stands as it is.
    pub path: Option<Vec<usize>>,
    /// Boxes laid out (the entry-point count across the relayout).
    pub visits: u64,
    /// Boxes moved by the delta, after the changed box.
    pub moved: u64,
    /// Boxes compared by the diff (no layout).
    pub compared: u64,
    /// The changed box's border-box bottom, new minus old.
    pub delta: f32,
}

/// Lay `fresh` out by updating `laid` (the layout of `old_unlaid`) in place.
///
/// On `Ok`, `laid` is what a full layout of `fresh` gives. On `Err`, `laid` is
/// unchanged and the caller must lay `fresh` out in full.
///
/// Taken only when the trees differ under one block box B and:
/// - B is an in-flow, static, unfloated, uncleared `display: block` box with
///   visible overflow and no percentage in its heights;
/// - no margin collapses through B's top or bottom edge (each is closed by a
///   border or padding, or the child next to it is inline-level, before and
///   after), and B is not empty before or after;
/// - B's subtree, old and new, holds no float, `clear`, non-static box or
///   percentage height;
/// - every ancestor is a plain auto-height block in flow (no table, flex,
///   grid, columns, `-webkit-box`, size container, transform, filter,
///   min/max height, non-visible overflow, vertical writing mode);
/// - no float before B reaches below B's top;
/// - every absolute box outside B has a positioned ancestor off the path
///   from the root to B, and no fixed box comes after B.
pub fn relayout(
    old_unlaid: &LayoutBox,
    fresh: &LayoutBox,
    laid: &mut LayoutBox,
) -> Result<Relaid, Fallback> {
    let mut out = Relaid::default();
    let path = match changed_path(old_unlaid, fresh, &mut out.compared)? {
        None => return Ok(out),
        Some(path) => path,
    };
    if path.is_empty() {
        return Err("root_box");
    }
    let old_b = at(laid, &path).ok_or("shape")?;
    let new_b = at(fresh, &path).ok_or("shape")?;
    let old_unlaid_b = at(old_unlaid, &path).ok_or("shape")?;

    changed_box_ok(new_b)?;
    subtree_ok(new_b)?;
    subtree_ok(old_b)?;
    let top_closed = edge_closed_top(old_b);
    let bottom_closed = edge_closed_bottom(old_b);
    for b in [new_b, old_unlaid_b] {
        if !top_closed && !b.children.first().is_some_and(is_inline_level) {
            return Err("margin_collapse_top");
        }
        if !bottom_closed && !b.children.last().is_some_and(is_inline_level) {
            return Err("margin_collapse_bottom");
        }
    }
    ancestors_ok(laid, &path)?;
    let b_top = old_b.dimensions.border_box().y;
    let mut scan = Scan {
        b_top,
        failure: None,
    };
    scan.walk(laid, &path, 0, false, false, false);
    if let Some(why) = scan.failure {
        return Err(why);
    }

    // Lay B out alone where the full pass put it: its top margin counts as
    // already adjoined (the context resolves to 0), so its content edge
    // lands at the old one.
    let parent = at(laid, &path[..path.len() - 1]).ok_or("shape")?;
    let mut nb = new_b.clone();
    nb.set_viewport(old_b.viewport.0, old_b.viewport.1);
    nb.percent_height_is_auto = old_b.percent_height_is_auto;
    let shares_floats = matches!(nb.box_type, BoxType::Block | BoxType::AnonymousBlock)
        && !crate::margin_collapse::establishes_bfc(&nb.style, nb.float);
    nb.context_floats = shares_floats.then(FloatContext::new);
    let d = &old_b.dimensions;
    let mut cb = parent.dimensions.clone();
    cb.content.x = d.content.x - d.margin.left - d.border_with_gutter().left - d.padding.left;
    cb.content.y = d.content.y - d.border_with_gutter().top - d.padding.top;
    cb.content.height = 0.0;
    let mut margins = MarginCollapseContext::new();
    margins.first_child_top_adjoined = true;
    let before = layout_visits();
    nb.layout_with_collapse_in(&cb, &mut margins, &mut FloatContext::new(), None);
    out.visits = layout_visits() - before;
    nb.context_floats = None;
    if margins.child_collapsed_through {
        return Err("collapse_through");
    }
    let (old_h, new_h) = (
        old_b.dimensions.border_box().height,
        nb.dimensions.border_box().height,
    );
    if (old_h <= 0.0 || new_h <= 0.0) && !(top_closed && bottom_closed) {
        return Err("collapse_through");
    }
    let nd = &nb.dimensions;
    if !close(nd.content.x, d.content.x)
        || !close(nd.content.y, d.content.y)
        || !close(nd.content.width, d.content.width)
        || !edges_close(&nd.margin, &d.margin)
    {
        return Err("geometry_moved");
    }
    out.delta = nb.dimensions.border_box().bottom() - old_b.dimensions.border_box().bottom();
    out.moved = splice(laid, &path, nb, out.delta);
    out.path = Some(path);
    Ok(out)
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() <= 0.01
}

fn edges_close(a: &EdgeSizes, b: &EdgeSizes) -> bool {
    close(a.top, b.top)
        && close(a.right, b.right)
        && close(a.bottom, b.bottom)
        && close(a.left, b.left)
}

fn at<'a>(root: &'a LayoutBox, path: &[usize]) -> Option<&'a LayoutBox> {
    let mut b = root;
    for &i in path {
        b = b.children.get(i)?;
    }
    Some(b)
}

/// Replace the box at `path` with `nb`, move every later sibling subtree at
/// each level by `delta`, and grow each ancestor by it. Returns how many
/// boxes moved.
fn splice(node: &mut LayoutBox, path: &[usize], nb: LayoutBox, delta: f32) -> u64 {
    let (&i, rest) = path.split_first().expect("non-empty path");
    let mut moved = if rest.is_empty() {
        node.children[i] = nb;
        0
    } else {
        splice(&mut node.children[i], rest, nb, delta)
    };
    if delta != 0.0 {
        for later in &mut node.children[i + 1..] {
            moved += count(later);
            LayoutBox::shift_subtree(later, 0.0, delta);
        }
        node.dimensions.content.height += delta;
    }
    moved
}

fn count(b: &LayoutBox) -> u64 {
    1 + b.children.iter().map(count).sum::<u64>()
}

/// The child-index path to the box whose children changed, `None` when the
/// trees are equal, or a fallback when the root itself changed.
fn changed_path(
    old: &LayoutBox,
    new: &LayoutBox,
    compared: &mut u64,
) -> Result<Option<Vec<usize>>, Fallback> {
    *compared += 1;
    if !own_inputs_equal(old, new) {
        return Err("root_changed");
    }
    let mut path = Vec::new();
    let (mut a, mut b) = (old, new);
    loop {
        if a.children.len() != b.children.len() {
            return Ok(Some(path));
        }
        let mut differing = None;
        let mut several = false;
        for (i, (ca, cb)) in a.children.iter().zip(&b.children).enumerate() {
            if !subtree_equal(ca, cb, compared) {
                if differing.is_some() {
                    several = true;
                    break;
                }
                differing = Some(i);
            }
        }
        let Some(i) = differing else {
            return Ok(if path.is_empty() { None } else { Some(path) });
        };
        if several {
            return Ok(Some(path));
        }
        let (ca, cb) = (&a.children[i], &b.children[i]);
        if matches!(cb.box_type, BoxType::Block | BoxType::AnonymousBlock)
            && own_inputs_equal(ca, cb)
        {
            path.push(i);
            a = ca;
            b = cb;
            continue;
        }
        return Ok(Some(path));
    }
}

fn subtree_equal(a: &LayoutBox, b: &LayoutBox, compared: &mut u64) -> bool {
    *compared += 1;
    own_inputs_equal(a, b)
        && a.children.len() == b.children.len()
        && a.children
            .iter()
            .zip(&b.children)
            .all(|(x, y)| subtree_equal(x, y, compared))
}

fn dbg_eq<T: std::fmt::Debug>(a: &T, b: &T) -> bool {
    format!("{a:?}") == format!("{b:?}")
}

fn rect_eq(a: &Rect, b: &Rect) -> bool {
    a.x == b.x && a.y == b.y && a.width == b.width && a.height == b.height
}

fn edges_eq(a: &EdgeSizes, b: &EdgeSizes) -> bool {
    a.top == b.top && a.right == b.right && a.bottom == b.bottom && a.left == b.left
}

fn dims_eq(a: &Dimensions, b: &Dimensions) -> bool {
    rect_eq(&a.content, &b.content)
        && edges_eq(&a.padding, &b.padding)
        && edges_eq(&a.border, &b.border)
        && edges_eq(&a.margin, &b.margin)
        && edges_eq(&a.scrollbar, &b.scrollbar)
}

fn box_type_eq(a: &BoxType, b: &BoxType) -> bool {
    match (a, b) {
        (BoxType::Block, BoxType::Block)
        | (BoxType::Inline, BoxType::Inline)
        | (BoxType::AnonymousBlock, BoxType::AnonymousBlock)
        | (BoxType::LineBreak, BoxType::LineBreak) => true,
        (BoxType::Text(x), BoxType::Text(y)) => x == y,
        (
            BoxType::Image {
                url: u1,
                natural_width: w1,
                natural_height: h1,
            },
            BoxType::Image {
                url: u2,
                natural_width: w2,
                natural_height: h2,
            },
        ) => u1 == u2 && w1 == w2 && h1 == h2,
        (BoxType::FormControl(x), BoxType::FormControl(y)) => dbg_eq(x, y),
        _ => false,
    }
}

/// Every field of a box that layout reads, children aside. The destructure
/// names every field, so a new one does not compile until it is classed
/// here as an input (compared) or an output (ignored).
fn own_inputs_equal(a: &LayoutBox, b: &LayoutBox) -> bool {
    let LayoutBox {
        box_type,
        dimensions,
        style,
        children: _,
        position,
        offsets,
        float,
        clear,
        z_index,
        stacking_context,
        containing_block_index,
        viewport,
        sticky_state,
        element_id,
        identity,
        link_href,
        focused_caret,
        focused_selection,
        text_sources,
        node_id,
        text_lines,
        text_flow_first_offset,
        percent_height_is_auto,
        root_element_height,
        table_span,
        table_caption_shift,
        context_floats,
        // A record of the last child layouts, never read by layout.
        layout_memo: _,
        // Outputs the engine stamps after layout; layout never reads them.
        element_scroll: _,
        sticky_rule: _,
        sticky_shift: _,
        sticky_paths: _,
        scrollbar_thumb_hovered: _,
        scrollbar_overflows,
        pseudo_animation_key,
    } = a;
    box_type_eq(box_type, &b.box_type)
        && *position == b.position
        && *float == b.float
        && *clear == b.clear
        && *z_index == b.z_index
        && *containing_block_index == b.containing_block_index
        && *viewport == b.viewport
        && *element_id == b.element_id
        && *identity == b.identity
        && *link_href == b.link_href
        && *focused_caret == b.focused_caret
        && *focused_selection == b.focused_selection
        && *text_sources == b.text_sources
        && *node_id == b.node_id
        && *text_flow_first_offset == b.text_flow_first_offset
        && *percent_height_is_auto == b.percent_height_is_auto
        && *root_element_height == b.root_element_height
        && *table_span == b.table_span
        && *table_caption_shift == b.table_caption_shift
        && *pseudo_animation_key == b.pseudo_animation_key
        && scrollbar_overflows.get() == b.scrollbar_overflows.get()
        && dims_eq(dimensions, &b.dimensions)
        && dbg_eq(offsets, &b.offsets)
        && stacking_context.is_some() == b.stacking_context.is_some()
        && (stacking_context.is_none() || dbg_eq(stacking_context, &b.stacking_context))
        && sticky_state.is_some() == b.sticky_state.is_some()
        && (sticky_state.is_none() || dbg_eq(sticky_state, &b.sticky_state))
        && text_lines.is_some() == b.text_lines.is_some()
        && (text_lines.is_none() || dbg_eq(text_lines, &b.text_lines))
        && context_floats.is_some() == b.context_floats.is_some()
        && (context_floats.is_none() || dbg_eq(context_floats, &b.context_floats))
        && **style == *b.style
}

fn is_inline_level(b: &LayoutBox) -> bool {
    b.style.display.is_atomic_inline()
        || matches!(
            b.box_type,
            BoxType::Inline
                | BoxType::Text(_)
                | BoxType::Image { .. }
                | BoxType::FormControl(_)
                | BoxType::LineBreak
        )
}

fn percent(l: &Length) -> bool {
    crate::flex::has_percentage(l)
}

fn changed_box_ok(b: &LayoutBox) -> Result<(), Fallback> {
    if !matches!(b.box_type, BoxType::Block | BoxType::AnonymousBlock)
        || b.style.display != Display::Block
    {
        return Err("changed_not_block");
    }
    if b.position != Position::Static || b.style.position != rustkit_css::Position::Static {
        return Err("changed_position");
    }
    if b.float != Float::None {
        return Err("changed_float");
    }
    if b.clear != Clear::None {
        return Err("changed_clear");
    }
    if percent(&b.style.height) || percent(&b.style.min_height) || percent(&b.style.max_height) {
        return Err("changed_percent_height");
    }
    if b.style.overflow_x != Overflow::Visible || b.style.overflow_y != Overflow::Visible {
        return Err("changed_overflow");
    }
    plain_container(b).map_err(|_| "changed_not_block")
}

/// A block whose children flow as plain blocks and lines.
fn plain_container(b: &LayoutBox) -> Result<(), Fallback> {
    let s = &b.style;
    if s.display != Display::Block
        || s.column_count.is_some()
        || s.columns_declared
        || s.webkit_box.is_some()
        || s.writing_mode != WritingMode::HorizontalTb
        || s.container_type != ContainerType::Normal
    {
        return Err("ancestor_not_block");
    }
    Ok(())
}

fn subtree_ok(b: &LayoutBox) -> Result<(), Fallback> {
    for child in &b.children {
        if matches!(child.position, Position::Absolute | Position::Fixed)
            || matches!(
                child.style.position,
                rustkit_css::Position::Absolute | rustkit_css::Position::Fixed
            )
        {
            return Err("subtree_out_of_flow");
        }
        if child.float != Float::None || child.clear != Clear::None {
            return Err("subtree_float");
        }
        if child.position != Position::Static
            || child.style.position != rustkit_css::Position::Static
        {
            return Err("subtree_positioned");
        }
        if percent(&child.style.height)
            || percent(&child.style.min_height)
            || percent(&child.style.max_height)
        {
            return Err("subtree_percent_height");
        }
        subtree_ok(child)?;
    }
    Ok(())
}

fn edge_closed_top(b: &LayoutBox) -> bool {
    b.dimensions.border_with_gutter().top > 0.0
        || b.dimensions.padding.top > 0.0
        || crate::margin_collapse::establishes_bfc(&b.style, b.float)
}

fn edge_closed_bottom(b: &LayoutBox) -> bool {
    crate::margin_collapse::establishes_bfc(&b.style, b.float)
        || !crate::margin_collapse::should_collapse_with_last_child(
            &b.style,
            b.float,
            b.dimensions.border_with_gutter().bottom,
            b.dimensions.padding.bottom,
        )
}

fn ancestors_ok(root: &LayoutBox, path: &[usize]) -> Result<(), Fallback> {
    let mut b = root;
    for &i in path {
        ancestor_ok(b)?;
        b = &b.children[i];
    }
    Ok(())
}

fn ancestor_ok(b: &LayoutBox) -> Result<(), Fallback> {
    if !matches!(b.box_type, BoxType::Block | BoxType::AnonymousBlock) {
        return Err("ancestor_not_block");
    }
    plain_container(b)?;
    let s = &b.style;
    if b.float != Float::None
        || !matches!(b.position, Position::Static | Position::Relative)
        || matches!(
            s.position,
            rustkit_css::Position::Absolute
                | rustkit_css::Position::Fixed
                | rustkit_css::Position::Sticky
        )
    {
        return Err("ancestor_out_of_flow");
    }
    if [&s.top, &s.bottom].into_iter().flatten().any(percent) {
        return Err("ancestor_out_of_flow");
    }
    let unset = |l: &Length| matches!(l, Length::Auto) || matches!(l, Length::Zero);
    if !matches!(s.height, Length::Auto)
        || !unset(&s.min_height)
        || !matches!(s.max_height, Length::Auto)
    {
        return Err("ancestor_sized");
    }
    if s.overflow_x != Overflow::Visible
        || s.overflow_y != Overflow::Visible
        || s.line_clamp().is_some()
    {
        return Err("ancestor_overflow");
    }
    if holds_fixed(s) {
        return Err("ancestor_holds_fixed");
    }
    Ok(())
}

/// Whatever makes a box the containing block of fixed descendants.
fn holds_fixed(s: &rustkit_css::ComputedStyle) -> bool {
    s.transform != rustkit_css::TransformList::default()
        || s.translate.is_some()
        || s.rotate.is_some()
        || s.scale.is_some()
        || !s.filter.is_empty()
        || !s.backdrop_filter.is_empty()
        || s.will_change_holds_fixed
        || s.contain_holds_fixed
}

/// The scan of the laid-out tree outside B: floats before B that reach it,
/// absolute boxes anchored on the path, fixed boxes after B.
struct Scan {
    b_top: f32,
    failure: Option<Fallback>,
}

impl Scan {
    /// `rest` is the path below `b` while `b` is on it (empty once off it).
    fn walk(
        &mut self,
        b: &LayoutBox,
        rest: &[usize],
        _depth: usize,
        off_path: bool,
        after: bool,
        held: bool,
    ) {
        if self.failure.is_some() {
            return;
        }
        let on_path = !off_path;
        if off_path {
            if b.float != Float::None
                && !after
                && b.dimensions.margin_box().bottom() > self.b_top + 0.01
            {
                self.failure = Some("float_beside");
                return;
            }
            match b.position {
                Position::Absolute if !held => {
                    self.failure = Some("abspos_on_path");
                    return;
                }
                Position::Fixed if after => {
                    self.failure = Some("fixed_after");
                    return;
                }
                _ => {}
            }
        }
        // Positioned off the path: what is under it is anchored off the path.
        let held_below = held || (off_path && b.position != Position::Static);
        if on_path && rest.is_empty() {
            // B itself: its subtree was checked on its own.
            return;
        }
        for (i, child) in b.children.iter().enumerate() {
            let (child_off, child_after, child_rest): (bool, bool, &[usize]) = if on_path {
                match i.cmp(&rest[0]) {
                    std::cmp::Ordering::Equal => (false, false, &rest[1..]),
                    std::cmp::Ordering::Less => (true, false, &[]),
                    std::cmp::Ordering::Greater => (true, true, &[]),
                }
            } else {
                (true, after, &[])
            };
            self.walk(
                child,
                child_rest,
                _depth + 1,
                child_off,
                child_after,
                held_below,
            );
        }
    }
}

/// Where two laid-out trees differ, in shapes only: each entry is the child
/// path of a box and what differs (`shape`, `content`, `padding`, `border`,
/// `margin`, `lines`). Rects compare to 0.01 px.
pub fn compare_rects(a: &LayoutBox, b: &LayoutBox, limit: usize) -> (usize, Vec<String>) {
    let mut count = 0;
    let mut shown = Vec::new();
    let mut path = Vec::new();
    compare_into(a, b, &mut path, &mut count, &mut shown, limit);
    (count, shown)
}

fn compare_into(
    a: &LayoutBox,
    b: &LayoutBox,
    path: &mut Vec<usize>,
    count: &mut usize,
    shown: &mut Vec<String>,
    limit: usize,
) {
    let mut what = Vec::new();
    let (da, db) = (&a.dimensions, &b.dimensions);
    let rect_close = |x: &Rect, y: &Rect| {
        close(x.x, y.x) && close(x.y, y.y) && close(x.width, y.width) && close(x.height, y.height)
    };
    if a.children.len() != b.children.len() || !box_type_eq(&a.box_type, &b.box_type) {
        what.push("shape");
    }
    if !rect_close(&da.content, &db.content) {
        what.push("content");
    }
    if !edges_close(&da.padding, &db.padding) {
        what.push("padding");
    }
    if !edges_close(&da.border, &db.border) || !edges_close(&da.scrollbar, &db.scrollbar) {
        what.push("border");
    }
    if !edges_close(&da.margin, &db.margin) {
        what.push("margin");
    }
    let lines = |l: &Option<Vec<crate::TextLine>>| {
        l.as_ref()
            .map(|v| v.iter().map(|t| (t.width, t.x_offset)).collect::<Vec<_>>())
    };
    let (la, lb) = (lines(&a.text_lines), lines(&b.text_lines));
    let lines_close = match (&la, &lb) {
        (None, None) => true,
        (Some(x), Some(y)) => {
            x.len() == y.len()
                && x.iter()
                    .zip(y)
                    .all(|(p, q)| close(p.0, q.0) && close(p.1, q.1))
        }
        _ => false,
    };
    if !lines_close {
        what.push("lines");
    }
    if !what.is_empty() {
        *count += 1;
        if shown.len() < limit {
            let kind = match &a.box_type {
                BoxType::Block => "block",
                BoxType::AnonymousBlock => "anon",
                BoxType::Inline => "inline",
                BoxType::Text(_) => "text",
                BoxType::Image { .. } => "image",
                BoxType::FormControl(_) => "control",
                BoxType::LineBreak => "br",
            };
            shown.push(format!("{path:?} {kind} {}", what.join("+")));
        }
    }
    if a.children.len() == b.children.len() {
        for (i, (x, y)) in a.children.iter().zip(&b.children).enumerate() {
            path.push(i);
            compare_into(x, y, path, count, shown, limit);
            path.pop();
        }
    }
}
