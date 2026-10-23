//! `@container` size queries (css-contain-3 §6) at style time.
//!
//! A rule inside `@container` applies to an element only where its
//! condition holds for the element's query container: the nearest ancestor
//! with a `container-type` other than `normal` (and the queried
//! `container-name`, when one is given). Chrome answers the query with the
//! container's laid-out size. The cascade here runs before layout, so the
//! walk carries an estimate of each box's content width instead: the
//! viewport at the root, then for each block-level box in flow its parent's
//! width less its margins, borders and padding, or its own definite width.
//! Where no estimate can be made (shrink-to-fit boxes, flex and grid items
//! without a definite width), a query on that container does not hold,
//! which is how the engine treated every `@container` rule before.
//!
//! Not covered: container-relative units (`cq*`), style queries, vertical
//! writing modes, and sizes that only layout knows (a flex item's flexed
//! width, a block's auto height).

use std::cell::RefCell;

use rustkit_css::{BoxSizing, ComputedStyle, ContainerType, Display, Float, Length, Position};

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
            frames: vec![Frame { width: Some(w), container: None, unknown_names: Vec::new() }],
        });
        Self { previous: STATE.with(|s| std::mem::replace(&mut *s.borrow_mut(), state)) }
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
    pub(crate) fn enter(style: &ComputedStyle, parent: Option<&ComputedStyle>) -> Self {
        let pushed = STATE.with(|s| {
            let mut s = s.borrow_mut();
            let Some(state) = s.as_mut() else { return false };
            let parent_width = state.frames.last().and_then(|f| f.width);
            let frame = frame_for(style, parent, parent_width, state.viewport);
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
        let Some(state) = s.as_ref() else { return false };
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
    parent_width: Option<f32>,
    viewport: (f32, f32),
) -> Frame {
    let width = content_width(style, parent, parent_width, viewport);
    let container = match style.container_type {
        ContainerType::Normal => None,
        kind => Some(width.map(|inline| Container {
            names: style.container_name.clone(),
            inline,
            block: match kind {
                ContainerType::Size => definite_height(style, viewport),
                _ => None,
            },
        })),
    };
    Frame { width, container, unknown_names: style.container_name.clone() }
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

fn content_width(
    style: &ComputedStyle,
    parent: Option<&ComputedStyle>,
    parent_width: Option<f32>,
    vp: (f32, f32),
) -> Option<f32> {
    match style.display {
        // Inline content (and a box that is replaced by its children) sits
        // in the parent's line boxes.
        Display::Inline | Display::Contents => return parent_width,
        Display::None => return None,
        _ => {}
    }
    let extras = horizontal_extras(style, parent_width, vp)?;
    let to_content = |border_box_or_content: f32| match style.box_sizing {
        BoxSizing::BorderBox => (border_box_or_content - extras).max(0.0),
        BoxSizing::ContentBox => border_box_or_content.max(0.0),
    };
    let definite = resolve(&style.width, style, parent_width, vp).map(to_content);
    let in_flex_or_grid = parent.is_some_and(|p| {
        matches!(p.display, Display::Flex | Display::InlineFlex | Display::Grid | Display::InlineGrid)
    });
    let out_of_flow = matches!(style.position, Position::Absolute | Position::Fixed)
        || style.float != Float::None;
    let block_level = matches!(style.display, Display::Block | Display::Flex | Display::Grid);
    let width = match definite {
        Some(w) => w,
        None if in_flex_or_grid || out_of_flow || !block_level => return None,
        None => {
            let margins = resolve(&style.margin_left, style, parent_width, vp).unwrap_or(0.0)
                + resolve(&style.margin_right, style, parent_width, vp).unwrap_or(0.0);
            (parent_width? - margins - extras).max(0.0)
        }
    };
    // min-width wins over max-width (CSS 2.1 §10.4).
    let max = resolve(&style.max_width, style, parent_width, vp).map(to_content);
    let min = resolve(&style.min_width, style, parent_width, vp).map(to_content);
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
