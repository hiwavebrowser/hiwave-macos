//! Layout constraints and fragments: the L0 slice of
//! `docs/LAYOUT_CONSTRAINTS_FRAGMENTS_2026-09-30.md`.
//!
//! A layout query takes a typed [`Constraint`] and returns an explicit
//! [`Fragment`]. The one query this slice moves is the intrinsic size of a
//! nested flex or grid item that holds text or a form control
//! ([`intrinsic_fragment`]): the subtree is laid out under the constraint
//! by the same flex and grid layout final layout uses, and the result is
//! the contribution. The estimators it replaces at that call site stay for
//! every item outside the class, and the [`differential`] keeps both
//! numbers visible.

use crate::{BoxType, LayoutBox, Position};
use rustkit_css::{FlexDirection, Length, WritingMode};
use std::cell::{Cell, RefCell};

/// A box length in fixed point, 1/64 CSS px (§5; Chromium's `LayoutUnit` is
/// the precedent for the split). Box and fragment sizes are written in this
/// unit. Glyph advances and text baselines are not: they stay `f32`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct LayoutUnit(i32);

impl LayoutUnit {
    /// Units per CSS px.
    pub const PER_PX: i32 = 64;
    pub const ZERO: LayoutUnit = LayoutUnit(0);

    /// The nearest unit to `px`. Not a number (and anything a cast would
    /// saturate) clamps instead of wrapping.
    pub fn from_px(px: f32) -> Self {
        let raw = (px * Self::PER_PX as f32).round();
        if raw.is_nan() {
            return LayoutUnit(0);
        }
        LayoutUnit(raw.clamp(i32::MIN as f32, i32::MAX as f32) as i32)
    }

    pub fn to_px(self) -> f32 {
        self.0 as f32 / Self::PER_PX as f32
    }

    /// The count of 1/64 px.
    pub fn raw(self) -> i32 {
        self.0
    }
}

/// One axis of a size: a number, or the statement that there is none.
/// `Definite(0)` is a definite zero. It is not a synonym for `Indefinite`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxisSize {
    Definite(LayoutUnit),
    Indefinite,
}

impl AxisSize {
    pub fn definite_px(px: f32) -> Self {
        AxisSize::Definite(LayoutUnit::from_px(px))
    }

    pub fn px(self) -> Option<f32> {
        match self {
            AxisSize::Definite(u) => Some(u.to_px()),
            AxisSize::Indefinite => None,
        }
    }
}

/// What the query asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeQuery {
    MinContent,
    MaxContent,
    FitContent {
        available: LayoutUnit,
    },
    /// Lay out under the constraint's definite sizes.
    Definite,
}

/// The input of a layout query (§1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Constraint {
    /// The box's own border-box inline size, where the caller has fixed it.
    pub inline_size: AxisSize,
    /// The box's own border-box block size, where the caller has fixed it.
    pub block_size: AxisSize,
    pub query: SizeQuery,
    /// The space the query may use, per axis.
    pub available_inline: AxisSize,
    pub available_block: AxisSize,
    /// What a percentage resolves against, per axis. A percentage against
    /// an indefinite basis stays a percentage.
    pub percentage_inline: AxisSize,
    pub percentage_block: AxisSize,
    pub writing_mode: WritingMode,
    /// Which box supplies the percentage basis and the available size
    /// (`LayoutBox::element_id`), where it has an element.
    pub containing_block: Option<usize>,
}

impl Constraint {
    /// A box whose inline size is fixed at `inline_px` (border box) and
    /// whose block size is to be found: the question a grid row asks of an
    /// item once the columns are sized.
    pub fn definite_inline(
        inline_px: f32,
        writing_mode: WritingMode,
        containing_block: Option<usize>,
    ) -> Self {
        let inline = AxisSize::definite_px(inline_px);
        Constraint {
            inline_size: inline,
            block_size: AxisSize::Indefinite,
            query: SizeQuery::Definite,
            available_inline: inline,
            available_block: AxisSize::Indefinite,
            percentage_inline: inline,
            percentage_block: AxisSize::Indefinite,
            writing_mode,
            containing_block,
        }
    }
}

/// An inline and a block size, in the box unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FragmentSize {
    pub inline: LayoutUnit,
    pub block: LayoutUnit,
}

/// The result of one layout query (§2).
#[derive(Debug, Clone, PartialEq)]
pub struct Fragment {
    pub border_box: FragmentSize,
    pub content_box: FragmentSize,
    /// The last-line baseline as a distance below the border-box top, in
    /// text precision. `None` when no in-flow descendant carries one.
    pub baseline: Option<f32>,
    /// The extent of the content measured from the padding-box origin: the
    /// padding box itself, or further where a descendant's border box
    /// leaves it. Content overflow only; ink overflow is a later field.
    pub content_overflow: FragmentSize,
    /// The constraint this fragment answers.
    pub constraint: Constraint,
    /// Child fragments. Empty in L0; inline fragments, floats and
    /// fragmentation are later slices and use this slot.
    pub children: Vec<Fragment>,
    /// The border-box block size before it was written in the box unit, so
    /// a 1/64 snap is not mistaken for a wrap.
    pub unsnapped_block_px: f32,
}

impl Fragment {
    /// The fragment of a box that has just been laid out under `constraint`.
    fn of_laid_out(b: &LayoutBox, constraint: Constraint) -> Fragment {
        let border = b.dimensions.border_box();
        let padding = b.dimensions.padding_box();
        let content = &b.dimensions.content;
        let mut right = padding.x + padding.width;
        let mut bottom = padding.y + padding.height;
        for child in &b.children {
            descendant_extent(child, &mut right, &mut bottom);
        }
        Fragment {
            border_box: FragmentSize {
                inline: LayoutUnit::from_px(border.width),
                block: LayoutUnit::from_px(border.height),
            },
            content_box: FragmentSize {
                inline: LayoutUnit::from_px(content.width),
                block: LayoutUnit::from_px(content.height),
            },
            baseline: b.inline_block_baseline_y().map(|y| y - border.y),
            content_overflow: FragmentSize {
                inline: LayoutUnit::from_px(right - padding.x),
                block: LayoutUnit::from_px(bottom - padding.y),
            },
            constraint,
            children: Vec::new(),
            unsnapped_block_px: border.height,
        }
    }
}

/// Grow `right`/`bottom` to cover the border boxes of an in-flow subtree.
fn descendant_extent(b: &LayoutBox, right: &mut f32, bottom: &mut f32) {
    if b.style.display == rustkit_css::Display::None
        || matches!(b.position, Position::Absolute | Position::Fixed)
    {
        return;
    }
    let bb = b.dimensions.border_box();
    *right = right.max(bb.x + bb.width);
    *bottom = bottom.max(bb.y + bb.height);
    for child in &b.children {
        descendant_extent(child, right, bottom);
    }
}

fn holds_text_or_control(b: &LayoutBox) -> bool {
    match &b.box_type {
        BoxType::Text(t) => !t.trim().is_empty(),
        BoxType::FormControl(_) => true,
        _ => b
            .children
            .iter()
            .any(|c| c.style.display != rustkit_css::Display::None && holds_text_or_control(c)),
    }
}

/// Whether `item` is in the L0 class: a row flex container or a grid
/// container, block size `auto`, holding text or a form control.
///
/// Out of the class, and so still on the estimators and the repair passes:
/// a percentage or other relative block size, an `aspect-ratio` item, an
/// out-of-flow item, a plain block, a replaced element, any writing mode
/// but `horizontal-tb`. A COLUMN flex container is out as well, for a
/// reason that is this engine's and not the design's: `layout_flex_container`
/// reads a column's main size from the height its caller left in the box,
/// so an indefinite block size cannot be put to it yet.
pub(crate) fn in_l0_class(item: &LayoutBox) -> bool {
    let s = &item.style;
    let container = if s.display.is_flex() {
        matches!(
            s.flex_direction,
            FlexDirection::Row | FlexDirection::RowReverse
        )
    } else {
        s.display.is_grid()
    };
    container
        && s.writing_mode == WritingMode::HorizontalTb
        && matches!(s.height, Length::Auto)
        && matches!(s.min_height, Length::Auto | Length::Px(_))
        && matches!(s.max_height, Length::Auto | Length::Px(_))
        && s.aspect_ratio.is_none()
        && !matches!(
            s.position,
            rustkit_css::Position::Absolute | rustkit_css::Position::Fixed
        )
        && holds_text_or_control(item)
}

thread_local! {
    /// Depth of `intrinsic_fragment` calls on this thread.
    static QUERY_DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// `RUSTKIT_L0=0` keeps every call site on the estimators (the A arm of an
/// A/B on one binary).
fn enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("RUSTKIT_L0").map_or(true, |v| v != "0"))
}

/// The L0 query: the fragment of `item` under `constraint`.
///
/// `item` is not modified. A copy of its subtree is given its box by
/// `place` (the caller's own placement step, the one its final layout
/// runs: margins, alignment, padding and border for a definite inline
/// size) and is then laid out by `layout_flex_container` or
/// `layout_grid_container`, the functions final layout calls.
///
/// `None` means "not this slice" and the caller keeps its present path:
/// the item is outside [`in_l0_class`], the constraint is one L0 does not
/// execute (anything but a definite inline size with an indefinite block
/// size, in `horizontal-tb`), or this call is already inside a query. That
/// last rule bounds the cost: a container nested in a queried subtree is
/// laid out once more by the query and not once more per level.
pub(crate) fn intrinsic_fragment(
    item: &LayoutBox,
    constraint: &Constraint,
    place: impl FnOnce(&mut LayoutBox),
) -> Option<Fragment> {
    if !enabled()
        || constraint.writing_mode != WritingMode::HorizontalTb
        || constraint.query != SizeQuery::Definite
        || constraint.block_size != AxisSize::Indefinite
        || !matches!(constraint.inline_size, AxisSize::Definite(_))
        || !in_l0_class(item)
        || QUERY_DEPTH.with(|d| d.get()) > 0
    {
        return None;
    }

    let mut probe = item.clone();
    place(&mut probe);
    QUERY_DEPTH.with(|d| d.set(d.get() + 1));
    if probe.style.display.is_flex() {
        let own_box = probe.dimensions.clone();
        crate::flex::layout_flex_container(&mut probe, &own_box);
    } else {
        let (w, h) = (
            probe.dimensions.content.width,
            probe.dimensions.content.height,
        );
        crate::grid::layout_grid_container(&mut probe, w, h);
    }
    QUERY_DEPTH.with(|d| d.set(d.get() - 1));

    Some(Fragment::of_laid_out(&probe, *constraint))
}

/// The four numbers of §4 for one in-slice item, less Chrome's: what the
/// estimate said, what the fragment says, and what the repair pass then
/// did to the item's row.
#[derive(Debug, Clone, PartialEq)]
pub struct Differential {
    /// The item's oracle selector, where it has an element.
    pub selector: Option<String>,
    /// `get_height_contribution`: outer height from `estimate_content_height`.
    pub old_estimate_px: f32,
    /// The fragment's border-box block size plus the item's block margins:
    /// the contribution track sizing used.
    pub fragment_outer_px: f32,
    /// The fragment's border-box block size, in the box unit.
    pub fragment_block: LayoutUnit,
    /// The same before the 1/64 snap.
    pub unsnapped_block_px: f32,
    /// What Phase 9.5 added to the item's row. `None` where the pass does
    /// not run (a grid with a definite height).
    pub phase_9_5_delta_px: Option<f32>,
}

thread_local! {
    static RECORD: RefCell<Option<Vec<Differential>>> = const { RefCell::new(None) };
}

fn log_to_stderr() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("RUSTKIT_L0_DIFF").is_some())
}

/// The differential of §4: collected on this thread between [`start`] and
/// [`take`], and printed one line per item when `RUSTKIT_L0_DIFF` is set.
///
/// [`start`]: differential::start
/// [`take`]: differential::take
pub mod differential {
    use super::{log_to_stderr, Differential, RECORD};

    /// Begin collecting on this thread.
    pub fn start() {
        RECORD.with(|r| *r.borrow_mut() = Some(Vec::new()));
    }

    /// Stop collecting and hand back what was recorded.
    pub fn take() -> Vec<Differential> {
        RECORD.with(|r| r.borrow_mut().take()).unwrap_or_default()
    }

    pub(crate) fn wanted() -> bool {
        log_to_stderr() || RECORD.with(|r| r.borrow().is_some())
    }

    pub(crate) fn record(d: Differential) {
        if log_to_stderr() {
            eprintln!(
                "L0 {} old_estimate={:.3} fragment_outer={:.3} fragment_block={}/64 unsnapped={:.5} phase_9_5_delta={}",
                d.selector.as_deref().unwrap_or("(anonymous)"),
                d.old_estimate_px,
                d.fragment_outer_px,
                d.fragment_block.raw(),
                d.unsnapped_block_px,
                d.phase_9_5_delta_px
                    .map_or("not-run".to_string(), |v| format!("{v:.3}")),
            );
        }
        RECORD.with(|r| {
            if let Some(v) = r.borrow_mut().as_mut() {
                v.push(d);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_layout_unit_is_a_sixty_fourth_of_a_pixel() {
        assert_eq!(LayoutUnit::from_px(1.0).raw(), 64);
        assert_eq!(LayoutUnit::from_px(43.0).to_px(), 43.0);
        // 0.01 px is 0.64 units: the nearest unit is 1.
        assert_eq!(LayoutUnit::from_px(0.01).raw(), 1);
        // Two ulps under a whole pixel is that pixel.
        assert_eq!(
            LayoutUnit::from_px(20.0 - 2.0 * f32::EPSILON * 20.0).raw(),
            20 * 64
        );
        assert_eq!(LayoutUnit::from_px(f32::NAN), LayoutUnit::ZERO);
        assert_eq!(LayoutUnit::from_px(f32::INFINITY).raw(), i32::MAX);
    }

    #[test]
    fn a_definite_zero_is_not_indefinite() {
        assert_ne!(AxisSize::definite_px(0.0), AxisSize::Indefinite);
        assert_eq!(AxisSize::definite_px(0.0).px(), Some(0.0));
        assert_eq!(AxisSize::Indefinite.px(), None);
    }

    #[test]
    fn a_query_l0_does_not_execute_is_not_this_slice() {
        let mut s = rustkit_css::ComputedStyle::new();
        s.display = rustkit_css::Display::Flex;
        let mut item = LayoutBox::new(BoxType::Block, s);
        item.children.push(LayoutBox::new(
            BoxType::Text("x".to_string()),
            rustkit_css::ComputedStyle::new(),
        ));
        assert!(in_l0_class(&item));

        let vertical = Constraint::definite_inline(100.0, WritingMode::VerticalRl, None);
        assert!(intrinsic_fragment(&item, &vertical, |_| {}).is_none());

        let mut min_content = Constraint::definite_inline(100.0, WritingMode::HorizontalTb, None);
        min_content.query = SizeQuery::MinContent;
        assert!(intrinsic_fragment(&item, &min_content, |_| {}).is_none());

        let mut column = item.clone();
        column.style.flex_direction = FlexDirection::Column;
        assert!(!in_l0_class(&column));

        let mut ratio = item.clone();
        ratio.style.aspect_ratio = Some(2.0);
        assert!(!in_l0_class(&ratio));

        let mut percent = item.clone();
        percent.style.height = Length::Percent(50.0);
        assert!(!in_l0_class(&percent));

        let mut empty = item.clone();
        empty.children.clear();
        assert!(!in_l0_class(&empty));
    }

    /// Membership that the first L0 slice pins in prose but not yet in a
    /// test: a form control counts; whitespace-only text, a `display:none`
    /// subtree, an image, and out-of-flow items do not. Nested queries also
    /// refuse to re-enter, so a queried subtree is not laid out once per level.
    #[test]
    fn l0_class_membership_and_nested_queries() {
        let flex = || {
            let mut s = rustkit_css::ComputedStyle::new();
            s.display = rustkit_css::Display::Flex;
            LayoutBox::new(BoxType::Block, s)
        };
        let mut with_control = flex();
        with_control.children.push(LayoutBox::new(
            BoxType::FormControl(crate::FormControlType::Button {
                label: "Go".into(),
                button_type: "button".into(),
            }),
            rustkit_css::ComputedStyle::new(),
        ));
        assert!(in_l0_class(&with_control), "a form control puts the item in L0");

        let mut grid = flex();
        grid.style.display = rustkit_css::Display::Grid;
        grid.children.push(LayoutBox::new(
            BoxType::Text("cell".into()),
            rustkit_css::ComputedStyle::new(),
        ));
        assert!(in_l0_class(&grid), "a grid holding text is in L0");

        let mut spaces = flex();
        spaces.children.push(LayoutBox::new(
            BoxType::Text(" \t\n".into()),
            rustkit_css::ComputedStyle::new(),
        ));
        assert!(!in_l0_class(&spaces), "whitespace-only text is not content");

        let mut hidden = flex();
        let mut none = rustkit_css::ComputedStyle::new();
        none.display = rustkit_css::Display::None;
        hidden.children.push(LayoutBox::new(BoxType::Text("x".into()), none));
        assert!(!in_l0_class(&hidden), "display:none children do not count");

        let mut image = flex();
        image.children.push(LayoutBox::new(
            BoxType::Image {
                url: "x.png".into(),
                natural_width: 10.0,
                natural_height: 10.0,
            },
            rustkit_css::ComputedStyle::new(),
        ));
        assert!(!in_l0_class(&image), "a replaced image alone is out of L0");

        let mut absolute = flex();
        absolute.style.position = rustkit_css::Position::Absolute;
        absolute.children.push(LayoutBox::new(
            BoxType::Text("x".into()),
            rustkit_css::ComputedStyle::new(),
        ));
        assert!(!in_l0_class(&absolute));
        absolute.style.position = rustkit_css::Position::Fixed;
        assert!(!in_l0_class(&absolute));

        let mut item = flex();
        item.children.push(LayoutBox::new(
            BoxType::Text("x".into()),
            rustkit_css::ComputedStyle::new(),
        ));
        let constraint = Constraint::definite_inline(100.0, WritingMode::HorizontalTb, None);
        QUERY_DEPTH.with(|d| d.set(1));
        assert!(
            intrinsic_fragment(&item, &constraint, |_| {}).is_none(),
            "a query already in flight must not re-enter"
        );
        QUERY_DEPTH.with(|d| d.set(0));
        assert!(
            intrinsic_fragment(&item, &constraint, |_| {}).is_some(),
            "precondition: the same item is queryable at depth 0"
        );
    }
}
