//! `@container` size queries (css-contain-3 §6) at style time.
//!
//! A rule inside `@container` applies to an element only where its
//! condition holds for the element's query container: the nearest ancestor
//! with a `container-type` other than `normal` (and the queried
//! `container-name`, when one is given). Chrome answers the query with the
//! container's laid-out size. The cascade here runs before layout, so the
//! walk carries an estimate of each box's content width instead.
//!
//! The estimate is an allowlist (#782 HOLD 3): a width is known only when
//! the container and every ancestor up to the root are plain in-flow
//! blocks, each with an auto width (its parent's width less its margins,
//! borders and padding) or a length or percentage of its parent's width.
//! A plain block here is `display: block` (which includes `list-item` and
//! `flow-root`), horizontal-tb, not floated or absolutely positioned, not a
//! `-webkit-box`, not a form control or replaced element, with no sizing
//! keyword (`min-content`, `fit-content`, ...) in `width`, `min-width` or
//! `max-width`, and not a child of a multi-column box. An auto-width box
//! that starts a formatting context after a float anywhere earlier in the
//! same formatting context is narrowed by it, so is unknown too.
//!
//! Everything else is unknown, and a query on an unknown container does not
//! hold, which is how the engine treated every `@container` rule before:
//! flex, grid and table descendants, inline and `display: contents`
//! containers, floats, absolute boxes, vertical writing modes. Some of
//! those have a width Chrome knows (a `flex: none` item, a table with a
//! fixed width); they stay false rather than risk a guess wider than the
//! laid-out box.
//!
//! Not covered: container-relative units (`cq*`), style queries, and sizes
//! that only layout knows (a block's auto height).

use std::cell::RefCell;

use rustkit_css::{
    BoxSizing, ComputedStyle, ContainerType, Display, Float, Length, Overflow, Position,
    WritingMode,
};

/// A query container, as an `@container` condition sees it.
#[derive(Debug, Clone)]
struct Container {
    names: Vec<String>,
    /// Content-box inline size, CSS px.
    inline: f32,
    /// Content-box block size; `None` unless `container-type: size` with a
    /// definite height.
    block: Option<f32>,
}

/// One element of the walk.
#[derive(Debug, Clone)]
struct Frame {
    /// Content-box width estimate of the element; `None` when unknown.
    width: Option<f32>,
    /// The element is a size container; `None` when it is not one, or when
    /// its size is unknown (`Some(None)` keeps the name so a named query
    /// still stops there and fails rather than reaching further out).
    container: Option<Option<Container>>,
    /// Names of an element that is a container of unknown size.
    unknown_names: Vec<String>,
    /// The element starts a block formatting context, which floats inside
    /// it do not leave.
    new_context: bool,
    /// A float has been entered in this box's formatting context, here or
    /// in an earlier descendant that does not start its own.
    floats: bool,
    /// Definite content-box height, when `height` is a length.
    height: Option<f32>,
    /// Element children entered so far.
    children: u32,
    /// The element was its parent's first element child.
    first: bool,
}

#[derive(Debug, Default)]
struct State {
    viewport: (f32, f32),
    frames: Vec<Frame>,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

/// Active for one box-tree build; restores the previous state on drop.
pub(crate) struct DocumentScope {
    previous: Option<State>,
}

impl DocumentScope {
    /// Installs the walk's state when there is a viewport and some rule
    /// carries a container condition; otherwise every such rule stays out.
    pub(crate) fn install(viewport: Option<(f32, f32)>, any_container_rule: bool) -> Self {
        let state = viewport.filter(|_| any_container_rule).map(|(w, h)| State {
            viewport: (w, h),
            frames: vec![Frame {
                width: Some(w),
                container: None,
                unknown_names: Vec::new(),
                new_context: true,
                floats: false,
                height: Some(h),
                children: 0,
                first: true,
            }],
        });
        Self {
            previous: STATE.with(|s| std::mem::replace(&mut *s.borrow_mut(), state)),
        }
    }
}

impl Drop for DocumentScope {
    fn drop(&mut self) {
        let previous = self.previous.take();
        STATE.with(|s| *s.borrow_mut() = previous);
    }
}

/// An element's frame; popped on drop. Its descendants (and its own
/// `::before`/`::after`) query through it.
pub(crate) struct ElementScope {
    pushed: bool,
}

impl ElementScope {
    pub(crate) fn enter(
        style: &ComputedStyle,
        parent: Option<&ComputedStyle>,
        tag: Option<&str>,
    ) -> Self {
        let pushed = STATE.with(|s| {
            let mut s = s.borrow_mut();
            let Some(state) = s.as_mut() else {
                return false;
            };
            let parent_width = state.frames.last().and_then(|f| f.width);
            // An earlier float in the same formatting context may sit beside
            // this box; one that starts a new context is then narrowed by
            // it, which only layout knows. (`clear` is not modelled.)
            let mut float_seen = false;
            for f in state.frames.iter().rev() {
                float_seen |= f.floats;
                if float_seen || f.new_context {
                    break;
                }
            }
            let mut frame = frame_for(style, parent, tag, parent_width, state.viewport, float_seen);
            if let Some(p) = state.frames.last_mut() {
                frame.first = p.children == 0;
                p.children += 1;
            }
            // A float overhangs a wrapper (block or inline) that does not
            // start a formatting context, so it still narrows a later box
            // there: record it up to the context's root. It stops short of
            // that at an ancestor of definite height no shorter than the
            // float when the float is that ancestor's leading content (it
            // and every box between are first children), so it sits at the
            // ancestor's top and cannot reach below it.
            if style.float != Float::None && style.display != Display::None {
                let float_h = margin_box_height(style, state.viewport);
                let mut leading = frame.first;
                for f in state.frames.iter_mut().rev() {
                    f.floats = true;
                    if f.new_context {
                        break;
                    }
                    if leading && float_h.is_some_and(|fh| f.height.is_some_and(|h| fh <= h)) {
                        break;
                    }
                    leading &= f.first;
                }
            }
            state.frames.push(frame);
            true
        });
        Self { pushed }
    }
}

impl Drop for ElementScope {
    fn drop(&mut self) {
        if self.pushed {
            STATE.with(|s| {
                if let Some(state) = s.borrow_mut().as_mut() {
                    state.frames.pop();
                }
            });
        }
    }
}

/// Whether a rule whose `@container` preludes (outermost first) are
/// `preludes` applies to the element now being styled.
pub(crate) fn applies(preludes: &[String]) -> bool {
    if preludes.is_empty() {
        return true;
    }
    STATE.with(|s| {
        let s = s.borrow();
        let Some(state) = s.as_ref() else {
            return false;
        };
        preludes.iter().all(|p| prelude_holds(p, &state.frames))
    })
}

fn prelude_holds(prelude: &str, frames: &[Frame]) -> bool {
    let prelude = prelude.trim();
    // `<container-name>? <container-query>`: a leading ident that is not
    // `not` and does not open a condition is the name.
    let (name, condition) = match prelude.split_once(|c: char| c.is_whitespace() || c == '(') {
        _ if prelude.starts_with('(') => (None, prelude),
        Some((first, _)) if !first.eq_ignore_ascii_case("not") && !first.is_empty() => {
            (Some(first), prelude[first.len()..].trim())
        }
        _ => (None, prelude),
    };
    for frame in frames.iter().rev() {
        let names: &[String] = match &frame.container {
            Some(Some(c)) => &c.names,
            Some(None) => &frame.unknown_names,
            None => continue,
        };
        if name.is_some_and(|n| !names.iter().any(|m| m == n)) {
            continue;
        }
        return match &frame.container {
            Some(Some(c)) => {
                rustkit_css::media::container_condition_matches(condition, c.inline, c.block)
            }
            _ => false,
        };
    }
    false
}

fn frame_for(
    style: &ComputedStyle,
    parent: Option<&ComputedStyle>,
    tag: Option<&str>,
    parent_width: Option<f32>,
    viewport: (f32, f32),
    float_seen: bool,
) -> Frame {
    let width = content_width(style, parent, tag, parent_width, viewport, float_seen);
    let container = match style.container_type {
        ContainerType::Normal => None,
        // An unknown width (every box that is not a plain block, among
        // them inline and `display: contents` ones) keeps the names, so a
        // named query stops here and fails.
        kind => Some(width.map(|inline| Container {
            names: style.container_name.clone(),
            inline,
            block: match kind {
                ContainerType::Size => definite_height(style, viewport),
                _ => None,
            },
        })),
    };
    Frame {
        width,
        container,
        unknown_names: style.container_name.clone(),
        new_context: starts_context(style),
        floats: false,
        height: definite_height(style, viewport),
        children: 0,
        first: true,
    }
}

/// Definite margin-box height (`height` a length; vertical margins,
/// padding and borders resolvable without a basis), else `None`.
fn margin_box_height(style: &ComputedStyle, vp: (f32, f32)) -> Option<f32> {
    let h = resolve(&style.height, style, None, vp)?;
    let v = |l: &Length| match l {
        Length::Auto => Some(0.0),
        l => resolve(l, style, None, vp),
    };
    let border = |w: &Length, s: rustkit_css::BorderStyle| match s {
        rustkit_css::BorderStyle::None => Some(0.0),
        _ => resolve(w, style, None, vp).or(Some(0.0)),
    };
    let around = v(&style.margin_top)?
        + v(&style.margin_bottom)?
        + border(&style.border_top_width, style.border_top_style)?
        + border(&style.border_bottom_width, style.border_bottom_style)?;
    Some(match style.box_sizing {
        BoxSizing::BorderBox => h.max(0.0) + v(&style.margin_top)? + v(&style.margin_bottom)?,
        BoxSizing::ContentBox => h + around + v(&style.padding_top)? + v(&style.padding_bottom)?,
    })
}

/// The box starts a block formatting context that floats do not leave (a
/// subset; missing some only makes more widths unknown).
fn starts_context(style: &ComputedStyle) -> bool {
    style.container_type != ContainerType::Normal
        || style.float != Float::None
        || matches!(style.position, Position::Absolute | Position::Fixed)
        || !matches!(style.overflow_x, Overflow::Visible | Overflow::Clip)
}

fn font_px(style: &ComputedStyle) -> f32 {
    match style.font_size {
        Length::Px(px) => px,
        _ => 16.0,
    }
}

/// A length that does not need layout to resolve; percentages resolve
/// against `basis`, and fail without one.
fn resolve(len: &Length, style: &ComputedStyle, basis: Option<f32>, vp: (f32, f32)) -> Option<f32> {
    let needs_basis = |l: &Length| matches!(l, Length::Percent(_));
    match len {
        Length::Auto | Length::FitContent => None,
        Length::Calc(..) | Length::Min(..) | Length::Max(..) | Length::Clamp(..) => {
            // Mixed math may hold a percentage; resolve against the basis,
            // or not at all when there is none.
            let b = basis?;
            Some(len.to_px_with_viewport(font_px(style), 16.0, b, vp.0, vp.1))
        }
        l if needs_basis(l) => {
            Some(l.to_px_with_viewport(font_px(style), 16.0, basis?, vp.0, vp.1))
        }
        l => Some(l.to_px_with_viewport(font_px(style), 16.0, 0.0, vp.0, vp.1)),
    }
}

fn horizontal_extras(style: &ComputedStyle, basis: Option<f32>, vp: (f32, f32)) -> Option<f32> {
    let border = |w: &Length, s: rustkit_css::BorderStyle| match s {
        rustkit_css::BorderStyle::None => Some(0.0),
        _ => resolve(w, style, None, vp).or(Some(0.0)),
    };
    Some(
        resolve(&style.padding_left, style, basis, vp)?
            + resolve(&style.padding_right, style, basis, vp)?
            + border(&style.border_left_width, style.border_left_style)?
            + border(&style.border_right_width, style.border_right_style)?,
    )
}

/// Shrink-to-fit form controls and replaced elements: their width is not
/// the parent's even with `display: block`.
const NOT_PLAIN_TAGS: &[&str] = &[
    "button", "input", "select", "textarea", "fieldset", "legend", "meter", "progress", "img",
    "video", "audio", "canvas", "iframe", "embed", "object", "svg", "math",
];

/// Floated or absolutely positioned: out of flow, so the block rules give
/// an auto or percentage width no meaning here, but a length is exact.
fn out_of_flow(style: &ComputedStyle) -> bool {
    style.float != Float::None || matches!(style.position, Position::Absolute | Position::Fixed)
}

/// A length with no percentage in it (`px`, `em`, `vw`, ...).
fn absolute_length(len: &Length) -> bool {
    !matches!(
        len,
        Length::Auto
            | Length::FitContent
            | Length::Percent(_)
            | Length::Calc(..)
            | Length::Min(..)
            | Length::Max(..)
            | Length::Clamp(..)
    )
}

/// A box whose width follows from its parent's by the block rules alone,
/// or (out of flow) from its own length `width`.
fn plain_block(style: &ComputedStyle, tag: Option<&str>) -> bool {
    style.display == Display::Block
        && style.webkit_box.is_none()
        && !style.display_contents_declared
        && (!out_of_flow(style)
            || (absolute_length(&style.width)
                && [&style.min_width, &style.max_width]
                    .iter()
                    .all(|l| matches!(l, Length::Auto) || absolute_length(l))))
        && style.writing_mode == WritingMode::HorizontalTb
        && !style.writing_mode_declared
        && style.width_keywords == 0
        && ![&style.width, &style.min_width, &style.max_width]
            .iter()
            .any(|l| matches!(l, Length::FitContent))
        && !tag.is_some_and(|t| NOT_PLAIN_TAGS.iter().any(|n| t.eq_ignore_ascii_case(n)))
}

/// Records a `width` / `min-width` / `max-width` declaration for the walk:
/// `bit` is set while the winning value is one `Length` cannot carry (the
/// content keywords in any case or with a `-webkit-` prefix), and cleared
/// by one it can (or `none`).
pub(crate) fn note_width_value(style: &mut ComputedStyle, bit: u8, parsed: bool, value: &str) {
    if parsed || value.trim().eq_ignore_ascii_case("none") {
        style.width_keywords &= !bit;
    } else {
        style.width_keywords |= bit;
    }
}

fn content_width(
    style: &ComputedStyle,
    parent: Option<&ComputedStyle>,
    tag: Option<&str>,
    parent_width: Option<f32>,
    vp: (f32, f32),
    float_seen: bool,
) -> Option<f32> {
    // Unknown below any box that is not a plain block (the parent's width
    // is then `None`), and in a multi-column box, whose children sit in
    // columns multicol layout sizes (`columns`/`column-width` are not
    // parsed yet but still make one).
    let parent_width = parent_width?;
    if !plain_block(style, tag)
        || parent.is_some_and(|p| p.column_count.is_some() || p.columns_declared)
    {
        return None;
    }
    let basis = Some(parent_width);
    let extras = horizontal_extras(style, basis, vp)?;
    let to_content = |border_box_or_content: f32| match style.box_sizing {
        BoxSizing::BorderBox => (border_box_or_content - extras).max(0.0),
        BoxSizing::ContentBox => border_box_or_content.max(0.0),
    };
    let width = match &style.width {
        // An earlier float in the same formatting context narrows a box
        // that starts its own; only layout knows by how much.
        Length::Auto if float_seen && starts_context(style) => return None,
        Length::Auto => {
            let margins = resolve(&style.margin_left, style, basis, vp).unwrap_or(0.0)
                + resolve(&style.margin_right, style, basis, vp).unwrap_or(0.0);
            (parent_width - margins - extras).max(0.0)
        }
        l => to_content(resolve(l, style, basis, vp)?),
    };
    // min-width wins over max-width (CSS 2.1 §10.4). A limit that does not
    // resolve here makes the width unknown rather than unlimited.
    let limit = |len: &Length| match len {
        Length::Auto => Some(None),
        l => resolve(l, style, basis, vp).map(|v| Some(to_content(v))),
    };
    let max = limit(&style.max_width)?;
    let min = limit(&style.min_width)?;
    let width = max.map_or(width, |m| width.min(m));
    Some(min.map_or(width, |m| width.max(m)))
}

fn definite_height(style: &ComputedStyle, vp: (f32, f32)) -> Option<f32> {
    // Percentages of the containing block's height need layout.
    let h = resolve(&style.height, style, None, vp)?;
    Some(match style.box_sizing {
        BoxSizing::BorderBox => {
            let v = |l: &Length| resolve(l, style, None, vp).unwrap_or(0.0);
            let border = |w: &Length, s: rustkit_css::BorderStyle| match s {
                rustkit_css::BorderStyle::None => 0.0,
                _ => v(w),
            };
            (h - v(&style.padding_top)
                - v(&style.padding_bottom)
                - border(&style.border_top_width, style.border_top_style)
                - border(&style.border_bottom_width, style.border_bottom_style))
            .max(0.0)
        }
        BoxSizing::ContentBox => h,
    })
}
