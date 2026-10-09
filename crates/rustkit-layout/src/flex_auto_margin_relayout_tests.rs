//! Auto margins on flex items whose sizes are only known after their children
//! are laid out. The flex pass sizes lines and items a second time then
//! (steps 11b and 11d), and the share an auto margin took in the first pass
//! must not be read as part of the item: §9.7 resolves the flexible lengths
//! with auto margins as 0. Found in review of #657; the first tests there all
//! gave their items a height, so the second pass never ran.

use super::*;
use rustkit_css::{Display, FlexDirection, FlexWrap};

fn block(height: Option<f32>, children: Vec<LayoutBox>) -> LayoutBox {
    let mut style = ComputedStyle::new();
    if let Some(h) = height {
        style.height = Length::Px(h);
    }
    let mut b = LayoutBox::new(BoxType::Block, style);
    b.children = children;
    b
}

fn flex(direction: FlexDirection, children: Vec<LayoutBox>) -> LayoutBox {
    let mut b = block(None, children);
    b.style.display = Display::Flex;
    b.style.flex_direction = direction;
    b
}

fn laid_out(mut root: LayoutBox) -> LayoutBox {
    root.set_viewport(1280.0, 800.0);
    let cb = Dimensions {
        content: Rect::new(0.0, 0.0, 600.0, 0.0),
        ..Default::default()
    };
    let mut mc = MarginCollapseContext::new();
    let mut fc = FloatContext::new();
    root.layout_with_collapse(&cb, &mut mc, &mut fc);
    root
}

fn y_and_height(b: &LayoutBox) -> (f32, f32) {
    (b.dimensions.content.y, b.dimensions.content.height)
}

/// `body { display: flex; flex-direction: column; min-height: 100vh }
/// footer { margin-top: auto }` with content above the footer.
#[test]
fn a_min_height_column_with_an_auto_margin_footer_is_its_min_height() {
    let content = block(None, vec![block(Some(50.0), vec![])]);
    let mut footer = block(Some(16.0), vec![]);
    footer.style.margin_top = Length::Auto;
    let mut column = flex(FlexDirection::Column, vec![content, footer]);
    column.style.min_height = Length::Px(200.0);

    let column = laid_out(column);
    let (footer_y, footer_h) = y_and_height(&column.children[1]);
    assert!(
        (column.dimensions.content.height - 200.0).abs() < 0.5,
        "the column is its min-height, 200, not {}",
        column.dimensions.content.height
    );
    assert!(
        (footer_y - 184.0).abs() < 0.5 && (footer_h - 16.0).abs() < 0.5,
        "the footer sits at the bottom: y {footer_y}, height {footer_h}"
    );
    assert!((column.children[1].dimensions.margin.top - 134.0).abs() < 0.5);
}

/// A definite column: the auto margin takes what is left after the items
/// have their sizes. It never shrinks them.
#[test]
fn an_auto_margin_does_not_shrink_the_items_of_a_definite_column() {
    let mut content = block(None, vec![block(Some(300.0), vec![])]);
    content.style.min_height = Length::Px(0.0);
    let mut footer = block(Some(50.0), vec![]);
    footer.style.margin_top = Length::Auto;
    let mut column = flex(FlexDirection::Column, vec![content, footer]);
    column.style.height = Length::Px(400.0);

    let column = laid_out(column);
    let (_, content_h) = y_and_height(&column.children[0]);
    let (footer_y, footer_h) = y_and_height(&column.children[1]);
    assert!((content_h - 300.0).abs() < 0.5, "the content keeps its 300, got {content_h}");
    assert!(
        (footer_y - 350.0).abs() < 0.5 && (footer_h - 50.0).abs() < 0.5,
        "the footer is 50 tall at y=350: y {footer_y}, height {footer_h}"
    );
}

/// The cross axis: a row's line is as tall as its tallest item after the
/// items are flowed, and the share a centred item's auto margins took in the
/// first pass is not part of that.
#[test]
fn a_rows_line_is_not_held_open_by_an_auto_cross_margin() {
    let tall = block(None, vec![block(Some(40.0), vec![])]);
    let mut centred = block(Some(20.0), vec![]);
    centred.style.width = Length::Px(30.0);
    centred.style.margin_top = Length::Auto;
    centred.style.margin_bottom = Length::Auto;
    let mut row = flex(FlexDirection::Row, vec![tall, centred]);
    row.style.flex_wrap = FlexWrap::Wrap;

    let row = laid_out(row);
    let (centred_y, _) = y_and_height(&row.children[1]);
    assert!(
        (row.dimensions.content.height - 40.0).abs() < 0.5,
        "the row is as tall as its 40px item, got {}",
        row.dimensions.content.height
    );
    assert!((centred_y - 10.0).abs() < 0.5, "the 20px item is centred at y=10, got {centred_y}");
    let m = &row.children[1].dimensions.margin;
    assert!((m.top - 10.0).abs() < 0.5 && (m.bottom - 10.0).abs() < 0.5);
}

/// The reverse directions (measured by the reviewer of #657 against §8.1):
/// the physical margin keeps its side.
#[test]
fn auto_margins_in_the_reverse_directions() {
    let item = |w: f32, h: f32| {
        let mut b = block(Some(h), vec![]);
        b.style.width = Length::Px(w);
        b
    };
    let mut second = item(100.0, 50.0);
    second.style.margin_left = Length::Auto;
    let mut row = flex(FlexDirection::RowReverse, vec![item(100.0, 50.0), second]);
    row.style.width = Length::Px(1000.0);
    let row = laid_out(row);
    let xs: Vec<f32> = row.children.iter().map(|c| c.dimensions.content.x).collect();
    assert!((xs[0] - 900.0).abs() < 0.5 && (xs[1] - 800.0).abs() < 0.5, "row-reverse: {xs:?}");
}
