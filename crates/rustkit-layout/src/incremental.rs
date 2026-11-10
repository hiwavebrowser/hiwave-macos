//! Incremental layout, stages 1 and 2 (`docs/design/INCREMENTAL_LAYOUT_PLAN.md`).
//!
//! Given the box tree the last layout started from (unlaid), the tree it
//! produced (laid out), and a freshly built unlaid tree, find the boxes
//! whose content changed. Each is hoisted to a relayout root R: the nearest
//! box at or above it that can be laid out alone (in flow, its margins
//! closed off or its edge children inline-level, nothing in it anchored
//! outside it) and whose ancestors up to a boundary only grow by R's
//! height delta. R is laid out alone where the full pass put it; what
//! follows it moves by the delta, and every ancestor up to the boundary
//! grows by it. A boundary is the root or a block whose height cannot
//! depend on its content; boxes after it do not move.
//!
//! Everything else returns a [`Fallback`] reason and the caller lays the
//! whole tree out, which is always correct. The engine's verify mode runs
//! both and compares every box rect ([`compare_rects`]).

use crate::{
    BoxType, Dimensions, EdgeSizes, FloatContext, LayoutBox, MarginCollapseContext, Position, Rect,
};
use rustkit_css::{Clear, ContainerType, Display, Float, Length, WritingMode};
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
    /// Child-index paths from the root to each relayout root laid out
    /// again, in document order; empty when nothing changed and the old
    /// layout stands as it is.
    pub roots: Vec<Vec<usize>>,
    /// Boxes laid out (the entry-point count across the relayout).
    pub visits: u64,
    /// Boxes moved by a delta, after a relayout root.
    pub moved: u64,
    /// Boxes compared by the diff (no layout).
    pub compared: u64,
    /// Levels the changed boxes were hoisted, in all.
    pub hoists: u32,
    /// Relayout roots whose delta stopped at a boundary below the root.
    pub bounded: u32,
    /// The sum of the roots' border-box bottom deltas.
    pub delta: f32,
}

/// One relayout root and where its delta stops.
#[derive(Debug, Clone)]
struct Plan {
    root: Vec<usize>,
    /// Depth of the boundary on the root's path, if one is below the
    /// tree's root. Ancestors deeper than it grow by the delta; it and they
    /// move the children after the path. `None`: every ancestor grows.
    stop: Option<usize>,
    hoists: u32,
}

/// Lay `fresh` out by updating `laid` (the layout of `old_unlaid`) in place.
///
/// On `Ok`, `laid` is what a full layout of `fresh` gives. On `Err`, `laid`
/// may be partly updated and must be dropped; the caller lays `fresh` out in
/// full.
pub fn relayout(
    old_unlaid: &LayoutBox,
    fresh: &LayoutBox,
    laid: &mut LayoutBox,
) -> Result<Relaid, Fallback> {
    let mut out = Relaid::default();
    let changed = nest_free(changed_roots(old_unlaid, fresh, &mut out.compared)?);
    if changed.is_empty() {
        return Ok(out);
    }
    let mut plans = Vec::new();
    for path in changed {
        plans.push(plan_for(laid, fresh, old_unlaid, path)?);
    }
    let roots = nest_free(plans.iter().map(|p| p.root.clone()).collect());
    plans.retain(|p| roots.contains(&p.root));
    plans.dedup_by(|a, b| a.root == b.root);
    plans.sort_by(|a, b| a.root.cmp(&b.root));
    // Last first: a root's delta then moves the later roots already laid
    // out, and each is laid out at its own old position.
    for plan in plans.iter().rev() {
        let (nb, delta) = lay_out_root(laid, fresh, plan, &mut out.visits)?;
        out.moved += splice(laid, &plan.root, 0, plan.stop, nb, delta)?;
        out.delta += delta;
        out.hoists += plan.hoists;
        out.bounded += u32::from(plan.stop.is_some());
    }
    out.roots = plans.into_iter().map(|p| p.root).collect();
    Ok(out)
}

/// Lay the box at `plan.root` out alone, from `fresh`, where the full pass
/// put it in `laid`. Returns the laid-out box and its border-box delta.
fn lay_out_root(
    laid: &LayoutBox,
    fresh: &LayoutBox,
    plan: &Plan,
    visits: &mut u64,
) -> Result<(LayoutBox, f32), Fallback> {
    let path = &plan.root;
    let old_b = at(laid, path).ok_or("shape")?;
    let new_b = at(fresh, path).ok_or("shape")?;
    let parent = at(laid, &path[..path.len() - 1]).ok_or("shape")?;
    let mut nb = new_b.clone();
    nb.set_viewport(old_b.viewport.0, old_b.viewport.1);
    nb.percent_height_is_auto = old_b.percent_height_is_auto;
    // Its parent's child loop hands an in-flow block that shares its
    // formatting context the floats so far; none reach it (`scan`).
    let shares_floats = matches!(nb.box_type, BoxType::Block | BoxType::AnonymousBlock)
        && !nb.style.display.is_atomic_inline()
        && !crate::margin_collapse::establishes_bfc(&nb.style, nb.float);
    nb.context_floats = shares_floats.then(FloatContext::new);
    // Its top margin counts as already adjoined (the context resolves to
    // 0), so its content edge lands at the old one.
    let d = &old_b.dimensions;
    let mut cb = parent.dimensions.clone();
    cb.content.x = d.content.x - d.margin.left - d.border_with_gutter().left - d.padding.left;
    cb.content.y = d.content.y - d.border_with_gutter().top - d.padding.top;
    cb.content.height = 0.0;
    let mut margins = MarginCollapseContext::new();
    margins.first_child_top_adjoined = true;
    let before = layout_visits();
    nb.layout_with_collapse_in(&cb, &mut margins, &mut FloatContext::new(), None);
    // The root pass's relative offsets, for what is under the box (its own
    // is in the old position it took).
    if !matches!(
        nb.box_type,
        BoxType::Inline | BoxType::Text(_) | BoxType::LineBreak
    ) {
        let inner = nb.dimensions.clone();
        for child in &mut nb.children {
            LayoutBox::shift_relative_subtree(child, &inner);
        }
    }
    *visits += layout_visits() - before;
    nb.context_floats = None;
    if margins.child_collapsed_through {
        return Err("collapse_through");
    }
    let closed = edge_closed_top(old_b) && edge_closed_bottom(old_b);
    let (old_h, new_h) = (d.border_box().height, nb.dimensions.border_box().height);
    if (old_h <= 0.0 || new_h <= 0.0) && !closed {
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
    let delta = nb.dimensions.border_box().bottom() - d.border_box().bottom();
    Ok((nb, delta))
}

/// Drop every path that has another (or an equal one) as a prefix, sorted.
fn nest_free(mut paths: Vec<Vec<usize>>) -> Vec<Vec<usize>> {
    paths.sort();
    let mut kept: Vec<Vec<usize>> = Vec::new();
    for p in paths {
        if !kept.iter().any(|k| p.starts_with(k)) {
            kept.push(p);
        }
    }
    kept
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

/// Replace the box at `path` with `nb`. At every level from the boundary
/// (`stop`) down, move the children after the path by `delta`; below the
/// boundary, grow the box by it. Returns how many boxes moved.
fn splice(
    node: &mut LayoutBox,
    path: &[usize],
    depth: usize,
    stop: Option<usize>,
    nb: LayoutBox,
    delta: f32,
) -> Result<u64, Fallback> {
    let (&i, rest) = path.split_first().expect("non-empty path");
    let mut moved = if rest.is_empty() {
        node.children[i] = nb;
        0
    } else {
        splice(&mut node.children[i], rest, depth + 1, stop, nb, delta)?
    };
    if delta != 0.0 && stop.map_or(true, |s| depth >= s) {
        for later in &mut node.children[i + 1..] {
            moved += count(later);
            LayoutBox::shift_subtree(later, 0.0, delta);
        }
        if stop.map_or(true, |s| depth > s) {
            let (old, new) = (
                node.dimensions.content.height,
                node.dimensions.content.height + delta,
            );
            let (min, max) = content_height_clamps(node);
            if min.is_some_and(|m| old <= m + 0.01 || new < m - 0.01)
                || max.is_some_and(|m| old >= m - 0.01 || new > m + 0.01)
            {
                return Err("ancestor_min_max");
            }
            node.dimensions.content.height = new;
        }
    }
    Ok(moved)
}

/// A growing ancestor's `min-height` and `max-height` as content heights
/// (`None` when unset). Percentages were refused by `classify_ancestor`.
fn content_height_clamps(b: &LayoutBox) -> (Option<f32>, Option<f32>) {
    let s = &b.style;
    let frame = if s.box_sizing == rustkit_css::BoxSizing::BorderBox {
        b.dimensions.padding.vertical() + b.dimensions.border_with_gutter().vertical()
    } else {
        0.0
    };
    let resolve = |l: &Length| match l {
        Length::Auto | Length::Zero => None,
        l => Some(b.length_to_px(l, 0.0) - frame),
    };
    (resolve(&s.min_height), resolve(&s.max_height))
}

fn count(b: &LayoutBox) -> u64 {
    1 + b.children.iter().map(count).sum::<u64>()
}

/// The child-index paths to the boxes whose children changed, or a
/// fallback when the root itself changed. Empty when the trees are equal.
fn changed_roots(
    old: &LayoutBox,
    new: &LayoutBox,
    compared: &mut u64,
) -> Result<Vec<Vec<usize>>, Fallback> {
    *compared += 1;
    if !own_inputs_equal(old, new) {
        return Err("root_changed");
    }
    let mut out = Vec::new();
    collect_changed(old, new, &mut Vec::new(), &mut out, compared);
    Ok(out)
}

fn collect_changed(
    a: &LayoutBox,
    b: &LayoutBox,
    path: &mut Vec<usize>,
    out: &mut Vec<Vec<usize>>,
    compared: &mut u64,
) {
    if a.children.len() != b.children.len() {
        out.push(path.clone());
        return;
    }
    let mut changed_here = false;
    for (i, (ca, cb)) in a.children.iter().zip(&b.children).enumerate() {
        if subtree_equal(ca, cb, compared) {
            continue;
        }
        if matches!(cb.box_type, BoxType::Block | BoxType::AnonymousBlock)
            && own_inputs_equal(ca, cb)
        {
            path.push(i);
            collect_changed(ca, cb, path, out, compared);
            path.pop();
        } else {
            changed_here = true;
        }
    }
    if changed_here {
        out.push(path.clone());
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

/// Hoist the changed box at `path` to a relayout root and find its
/// boundary. Each failed condition moves the root one level up; at the
/// tree's root, or its first level, the full layout is cheaper.
fn plan_for(
    laid: &LayoutBox,
    fresh: &LayoutBox,
    old_unlaid: &LayoutBox,
    path: Vec<usize>,
) -> Result<Plan, Fallback> {
    let mut root = path;
    let mut hoists = 0;
    let mut why: Fallback = if root.len() == 1 {
        "body_box"
    } else {
        "root_box"
    };
    'hoist: loop {
        if root.len() <= 1 {
            return Err(if root.is_empty() { "root_box" } else { why });
        }
        let (Some(l), Some(n), Some(u)) =
            (at(laid, &root), at(fresh, &root), at(old_unlaid, &root))
        else {
            return Err("shape");
        };
        if let Err(w) = root_ok(l, n, u) {
            why = w;
            root.pop();
            hoists += 1;
            continue;
        }
        let mut stop = None;
        for depth in (0..root.len()).rev() {
            let a = at(laid, &root[..depth]).ok_or("shape")?;
            let parent = depth.checked_sub(1).and_then(|p| at(laid, &root[..p]));
            match classify_ancestor(a, parent) {
                Ancestor::Grows => {}
                Ancestor::Boundary => {
                    stop = Some(depth);
                    break;
                }
                Ancestor::Hoist(w) => {
                    why = w;
                    hoists += (root.len() - depth) as u32;
                    root.truncate(depth);
                    continue 'hoist;
                }
            }
        }
        if let Err(w) = scan(laid, &root, stop) {
            why = w;
            root.pop();
            hoists += 1;
            continue;
        }
        return Ok(Plan { root, stop, hoists });
    }
}

/// Whether the box at a root's path can be laid out alone: `l` laid out,
/// `n` as built now, `u` as built last time.
fn root_ok(l: &LayoutBox, n: &LayoutBox, u: &LayoutBox) -> Result<(), Fallback> {
    let s = &n.style;
    if !matches!(n.box_type, BoxType::Block | BoxType::AnonymousBlock)
        || !matches!(s.display, Display::Block | Display::Flex | Display::Grid)
    {
        return Err("changed_not_block");
    }
    if l.position != Position::Static
        || !matches!(
            s.position,
            rustkit_css::Position::Static | rustkit_css::Position::Relative
        )
    {
        return Err("changed_position");
    }
    if [&s.top, &s.bottom, &s.left, &s.right]
        .into_iter()
        .flatten()
        .any(percent)
    {
        return Err("changed_position");
    }
    if n.float != Float::None {
        return Err("changed_float");
    }
    if n.clear != Clear::None {
        return Err("changed_clear");
    }
    if percent(&s.height) || percent(&s.min_height) || percent(&s.max_height) {
        return Err("changed_percent_height");
    }
    if s.column_count.is_some()
        || s.columns_declared
        || s.webkit_box.is_some()
        || s.writing_mode != WritingMode::HorizontalTb
    {
        return Err("changed_not_block");
    }
    let bfc = crate::margin_collapse::establishes_bfc(s, n.float);
    if !bfc {
        let top = edge_closed_top(l);
        let bottom = edge_closed_bottom(l);
        for b in [n, u] {
            if !top && !b.children.first().is_some_and(is_inline_level) {
                return Err("margin_collapse_top");
            }
            if !bottom && !b.children.last().is_some_and(is_inline_level) {
                return Err("margin_collapse_bottom");
            }
        }
    }
    subtree_ok(n, bfc)?;
    subtree_ok(l, bfc)
}

/// What is under a relayout root must lay out inside it: no fixed box, and
/// no float or `clear` unless the root contains them (a formatting root).
/// An absolute box anchors to its parent's padding box
/// (`reanchor_absolute_children`), which is inside the root.
fn subtree_ok(b: &LayoutBox, contains_floats: bool) -> Result<(), Fallback> {
    for child in &b.children {
        if child.position == Position::Fixed || child.style.position == rustkit_css::Position::Fixed
        {
            return Err("subtree_fixed");
        }
        if (child.float != Float::None || child.clear != Clear::None) && !contains_floats {
            return Err("subtree_float");
        }
        let inner =
            contains_floats || crate::margin_collapse::establishes_bfc(&child.style, child.float);
        subtree_ok(child, inner)?;
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

enum Ancestor {
    /// An auto-height block in flow: grows by the delta.
    Grows,
    /// A block in flow whose height cannot depend on its content: the
    /// delta moves its later children and stops there.
    Boundary,
    /// Neither: the relayout root moves up to it.
    Hoist(Fallback),
}

fn classify_ancestor(a: &LayoutBox, parent: Option<&LayoutBox>) -> Ancestor {
    let s = &a.style;
    if !matches!(a.box_type, BoxType::Block | BoxType::AnonymousBlock) {
        return Ancestor::Hoist("ancestor_not_block");
    }
    if s.display != Display::Block {
        return Ancestor::Hoist(match s.display {
            Display::Flex | Display::InlineFlex => "ancestor_flex",
            Display::Grid | Display::InlineGrid => "ancestor_grid",
            d if d.is_table() => "ancestor_table",
            _ => "ancestor_not_block",
        });
    }
    if s.column_count.is_some()
        || s.columns_declared
        || s.webkit_box.is_some()
        || s.line_clamp().is_some()
    {
        return Ancestor::Hoist("ancestor_columns");
    }
    if s.writing_mode != WritingMode::HorizontalTb {
        return Ancestor::Hoist("ancestor_not_block");
    }
    if s.container_type != ContainerType::Normal {
        return Ancestor::Hoist("ancestor_container");
    }
    let out_of_flow = a.float != Float::None
        || matches!(a.position, Position::Absolute | Position::Fixed)
        || matches!(
            s.position,
            rustkit_css::Position::Absolute | rustkit_css::Position::Fixed
        );
    if out_of_flow {
        return Ancestor::Hoist("ancestor_out_of_flow");
    }
    if [&s.top, &s.bottom].into_iter().flatten().any(percent) {
        return Ancestor::Hoist("ancestor_out_of_flow");
    }
    // In flow with its width from its containing block: a height that does
    // not read its content makes it a boundary.
    let fixed_height = match &s.height {
        Length::Auto | Length::FitContent => false,
        Length::Min(_) | Length::Max(_) | Length::Clamp(_) => false,
        l if percent(l) => {
            !a.percent_height_computes_to_auto()
                && matches!(s.height, Length::Percent(_))
                && parent
                    .is_some_and(|p| p.definite_content_height_for_children(0.0, false).is_some())
        }
        _ => true,
    };
    if fixed_height {
        return Ancestor::Boundary;
    }
    let auto_height = matches!(s.height, Length::Auto) || a.percent_height_computes_to_auto();
    if !auto_height {
        return Ancestor::Hoist("ancestor_sized");
    }
    // A min- or max-height clamps the content height; `splice` checks the
    // old and new heights are both on the content side of it.
    let keyword = |l: &Length| {
        matches!(
            l,
            Length::FitContent | Length::Min(_) | Length::Max(_) | Length::Clamp(_)
        )
    };
    if percent(&s.min_height)
        || percent(&s.max_height)
        || keyword(&s.min_height)
        || keyword(&s.max_height)
    {
        return Ancestor::Hoist("ancestor_min_max");
    }
    Ancestor::Grows
}

/// The laid-out tree outside the root `r`, against its boundary at depth
/// `stop`: no float before the root reaches below its top; no absolute or
/// fixed box is a child of a growing ancestor, or follows the path in a
/// box whose later children move; no fixed box is among what moves; no
/// box that moves or sits in a growing box has percentage offsets.
fn scan(laid: &LayoutBox, r: &[usize], stop: Option<usize>) -> Result<(), Fallback> {
    let grows = |depth: usize| stop.map_or(true, |s| depth > s);
    let r_top = at(laid, r).ok_or("shape")?.dimensions.border_box().y;
    floats_before(laid, r, r_top)?;
    for depth in stop.unwrap_or(0)..r.len() {
        let node = at(laid, &r[..depth]).ok_or("shape")?;
        let on_path = r[depth];
        for (i, c) in node.children.iter().enumerate() {
            if i == on_path {
                continue;
            }
            let oof = matches!(c.position, Position::Absolute | Position::Fixed)
                || matches!(
                    c.style.position,
                    rustkit_css::Position::Absolute | rustkit_css::Position::Fixed
                );
            if oof && (grows(depth) || i > on_path) {
                return Err("abspos_on_path");
            }
            if grows(depth)
                && [&c.style.top, &c.style.bottom]
                    .into_iter()
                    .flatten()
                    .any(percent)
            {
                return Err("relative_percent");
            }
            if i > on_path && contains_fixed(c) {
                return Err("fixed_after");
            }
        }
    }
    Ok(())
}

fn contains_fixed(b: &LayoutBox) -> bool {
    b.position == Position::Fixed
        || b.style.position == rustkit_css::Position::Fixed
        || b.children.iter().any(contains_fixed)
}

/// No float before the root in document order (outside its ancestors)
/// reaches below its top edge.
fn floats_before(b: &LayoutBox, rest: &[usize], r_top: f32) -> Result<(), Fallback> {
    let Some((&i, below)) = rest.split_first() else {
        return Ok(());
    };
    for c in &b.children[..i] {
        any_float_below(c, r_top)?;
    }
    floats_before(&b.children[i], below, r_top)
}

fn any_float_below(b: &LayoutBox, r_top: f32) -> Result<(), Fallback> {
    if b.float != Float::None && b.dimensions.margin_box().bottom() > r_top + 0.01 {
        return Err("float_beside");
    }
    b.children
        .iter()
        .try_for_each(|c| any_float_below(c, r_top))
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
            let (c, d) = (da.content, db.content);
            shown.push(format!(
                "{path:?} {kind} {} ({:.1},{:.1} {:.1}x{:.1} vs {:.1},{:.1} {:.1}x{:.1})",
                what.join("+"),
                c.x,
                c.y,
                c.width,
                c.height,
                d.x,
                d.y,
                d.width,
                d.height
            ));
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
