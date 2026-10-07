//! CSS 2.1 §10.5 and css-flexbox-1 §9.4.11 / §9.8: a percentage height
//! resolves against a containing block whose height is DEFINITE, and a flex
//! item's height is definite once the flex algorithm has fixed it (stretched
//! in a definite single line, or flexed along a definite column). Reduced from
//! ebay's search form (hand test H15), whose `height: 100%` input sat 22.8px
//! tall in a 40px slot. Every expected number was measured on the oracle
//! Chromium 143 at a 400px viewport (`getBoundingClientRect`, shapes P1 to P26
//! of the PR); each case runs through both entry points, `layout` and
//! `layout_with_collapse`.

use super::*;
use rustkit_css::{AlignItems, Display, FlexBasis, FlexDirection};

fn block() -> ComputedStyle {
    ComputedStyle::new()
}

fn sized(height: Length) -> ComputedStyle {
    let mut s = block();
    s.height = height;
    s
}

fn flex(direction: FlexDirection, height: Length) -> ComputedStyle {
    let mut s = sized(height);
    s.display = Display::Flex;
    s.flex_direction = direction;
    s
}

fn grow() -> ComputedStyle {
    let mut s = block();
    s.flex_grow = 1.0;
    s.flex_shrink = 1.0;
    s.flex_basis = FlexBasis::Percent(0.0);
    s
}

fn boxed(style: ComputedStyle, children: Vec<LayoutBox>) -> LayoutBox {
    let mut b = LayoutBox::new(BoxType::Block, style);
    b.children = children;
    b
}

/// `<div style="height: 100%">`, empty.
fn fill() -> LayoutBox {
    boxed(sized(Length::Percent(100.0)), Vec::new())
}

/// `<input style="height: <height>; width: 100%; border: none; padding: 0">`.
fn input(height: Length) -> LayoutBox {
    let mut s = ComputedStyle::new();
    s.display = Display::InlineBlock;
    s.font_family = "Arial".to_string();
    s.font_size = Length::Px(13.333);
    s.width = Length::Percent(100.0);
    s.height = height;
    LayoutBox::new(
        BoxType::FormControl(FormControlType::TextInput {
            value: String::new(),
            placeholder: String::new(),
            input_type: "text".to_string(),
        }),
        s,
    )
}

/// Lays `root` out in a block `<body>` 400px wide through the named entry
/// point and returns it.
fn laid_out(root: LayoutBox, collapse: bool) -> LayoutBox {
    let mut body = LayoutBox::new(BoxType::Block, ComputedStyle::new());
    body.children.push(root);
    let cb = Dimensions {
        content: Rect::new(0.0, 0.0, 400.0, 0.0),
        ..Default::default()
    };
    if collapse {
        let mut mc = MarginCollapseContext::new();
        let mut fc = FloatContext::new();
        body.layout_with_collapse(&cb, &mut mc, &mut fc);
    } else {
        body.layout(&cb);
    }
    body.children.remove(0)
}

/// The height of the box reached by taking the first child `depth` times.
fn height_at(root: &LayoutBox, depth: usize) -> f32 {
    let mut b = root;
    for _ in 0..depth {
        b = &b.children[0];
    }
    b.dimensions.content.height
}

/// Asserts the height `depth` first-children down, through both entry points.
fn assert_height(name: &str, build: impl Fn() -> LayoutBox, depth: usize, want: f32) {
    for collapse in [false, true] {
        let got = height_at(&laid_out(build(), collapse), depth);
        assert!(
            (got - want).abs() < 0.5,
            "{name} (collapse = {collapse}): height {got}, Chromium has {want}"
        );
    }
}

// ---- a control's percentage height -------------------------------------

#[test]
fn a_controls_percentage_height_resolves_against_a_definite_block() {
    // P5: block 44 > div 100% > input 100%.
    assert_height(
        "input 100% in a 100% block in a 44px block",
        || {
            boxed(
                sized(Length::Px(44.0)),
                vec![boxed(
                    sized(Length::Percent(100.0)),
                    vec![input(Length::Percent(100.0))],
                )],
            )
        },
        2,
        44.0,
    );
    // P19: block 44 > input 50%.
    assert_height(
        "input 50% in a 44px block",
        || boxed(sized(Length::Px(44.0)), vec![input(Length::Percent(50.0))]),
        1,
        22.0,
    );
}

#[test]
fn a_controls_percentage_height_is_auto_in_a_content_sized_block() {
    // P15 and P16: the parent's height depends on its content, so the
    // percentage is `auto` whatever has been laid out above the control. The
    // flow cursor used to be read as the base: 100% of a 30px sibling.
    let auto = height_at(
        &laid_out(boxed(block(), vec![input(Length::Auto)]), false),
        1,
    );
    for collapse in [false, true] {
        let alone = laid_out(boxed(block(), vec![input(Length::Percent(100.0))]), collapse);
        let got = alone.children[0].dimensions.content.height;
        assert!(
            (got - auto).abs() < 0.01,
            "alone (collapse = {collapse}): {got}, the auto control is {auto}"
        );
        let after = laid_out(
            boxed(
                block(),
                vec![
                    boxed(sized(Length::Px(30.0)), Vec::new()),
                    input(Length::Percent(100.0)),
                ],
            ),
            collapse,
        );
        let got = after.children[1].dimensions.content.height;
        assert!(
            (got - auto).abs() < 0.01,
            "after a 30px block (collapse = {collapse}): {got}, the auto control is {auto}"
        );
    }
}

// ---- inside a flex item whose height the flex algorithm fixed -----------

#[test]
fn a_percentage_height_resolves_against_a_stretched_row_item() {
    // P3: row 44 > flex:1 > div 100%.
    assert_height(
        "div 100% in a stretched item of a 44px row",
        || {
            boxed(
                flex(FlexDirection::Row, Length::Px(44.0)),
                vec![boxed(grow(), vec![fill()])],
            )
        },
        2,
        44.0,
    );
    // P1: row 44 > flex:1 > input 100%.
    assert_height(
        "input 100% in a stretched item of a 44px row",
        || {
            boxed(
                flex(FlexDirection::Row, Length::Px(44.0)),
                vec![boxed(grow(), vec![input(Length::Percent(100.0))])],
            )
        },
        2,
        44.0,
    );
    // P6: row 44 > flex:1 > div 100% > input 100%.
    assert_height(
        "input 100% in a 100% block in a stretched item",
        || {
            boxed(
                flex(FlexDirection::Row, Length::Px(44.0)),
                vec![boxed(
                    grow(),
                    vec![boxed(
                        sized(Length::Percent(100.0)),
                        vec![input(Length::Percent(100.0))],
                    )],
                )],
            )
        },
        3,
        44.0,
    );
}

#[test]
fn a_percentage_height_resolves_through_a_nested_stretched_flex_item() {
    // P11, ebay's shape: row 44 > flex (flex:1) > flex:1 > input 100%.
    assert_height(
        "input 100% two flex levels under a 44px row",
        || {
            let mut inner = grow();
            inner.display = Display::Flex;
            boxed(
                flex(FlexDirection::Row, Length::Px(44.0)),
                vec![boxed(
                    inner,
                    vec![boxed(grow(), vec![input(Length::Percent(100.0))])],
                )],
            )
        },
        3,
        44.0,
    );
}

#[test]
fn a_percentage_height_resolves_against_a_flexed_column_item() {
    // P9: column 200 > flex:1 > div 100%.
    assert_height(
        "div 100% in a flexed item of a 200px column",
        || {
            boxed(
                flex(FlexDirection::Column, Length::Px(200.0)),
                vec![boxed(grow(), vec![fill()])],
            )
        },
        2,
        200.0,
    );
}

#[test]
fn a_percentage_height_stays_auto_in_an_item_the_flex_algorithm_did_not_fix() {
    // P8: `align-items: center` does not stretch, so the item is as tall as
    // its content and the percentage is `auto`.
    assert_height(
        "div 100% in a centred item of a 44px row",
        || {
            let mut row = flex(FlexDirection::Row, Length::Px(44.0));
            row.align_items = AlignItems::Center;
            boxed(row, vec![boxed(grow(), vec![fill()])])
        },
        2,
        0.0,
    );
    // P25: a column item that does not flex is content-sized.
    assert_height(
        "div 100% in an inflexible item of a 200px column",
        || {
            boxed(
                flex(FlexDirection::Column, Length::Px(200.0)),
                vec![boxed(block(), vec![fill()])],
            )
        },
        2,
        0.0,
    );
    // P26: a flexing item of an auto-height column has nothing to grow into.
    assert_height(
        "div 100% in a flex:1 item of an auto-height column",
        || {
            boxed(
                flex(FlexDirection::Column, Length::Auto),
                vec![
                    boxed(grow(), vec![fill()]),
                    boxed(sized(Length::Px(30.0)), Vec::new()),
                ],
            )
        },
        2,
        0.0,
    );
}
