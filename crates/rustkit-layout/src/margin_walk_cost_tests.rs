//! LAT #1166: the margin-collapse walk on a deep chain of open blocks must
//! not grow faster than about the square of the depth. The engine pin in
//! `deep_relayout_cost_tests` needs `--features headless`; this is the same
//! counter (`margin_walks`) on a pure layout tree for default `--lib`.

use super::*;

fn open_block() -> LayoutBox {
    let mut s = ComputedStyle::new();
    s.margin_top = Length::Px(0.0);
    s.margin_bottom = Length::Px(0.0);
    LayoutBox::new(BoxType::Block, s)
}

fn leaf() -> LayoutBox {
    let mut s = ComputedStyle::new();
    s.height = Length::Px(10.0);
    LayoutBox::new(BoxType::Block, s)
}

/// `depth` nested open wrappers around one sized leaf (the HTML chain in the
/// headless LAT pin, without going through the engine).
fn deep_chain(depth: usize) -> LayoutBox {
    let mut cur = leaf();
    for _ in 0..depth {
        let mut wrap = open_block();
        wrap.children.push(cur);
        cur = wrap;
    }
    cur
}

fn layout(root: &mut LayoutBox) {
    let cb = Dimensions {
        content: Rect::new(0.0, 0.0, 800.0, 0.0),
        ..Default::default()
    };
    let mut mc = MarginCollapseContext::new();
    let mut fc = FloatContext::new();
    root.layout_with_collapse(&cb, &mut mc, &mut fc);
}

fn margin_walks_for(depth: usize) -> u64 {
    let mut root = deep_chain(depth);
    let before = margin_walks();
    layout(&mut root);
    margin_walks() - before
}

/// Each open wrapper's top-margin chain walks the chain below it. Doubling the
/// depth must not more than about quadruple that work (it was ×16 before the
/// collapse memo in #1166).
#[test]
fn the_margin_walk_grows_no_faster_than_the_square_of_the_depth() {
    // A debug build of a 100-deep box tree overflows the default test stack
    // (same reason the headless pin uses a large-stack thread).
    let (at_50, at_100) = std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(|| (margin_walks_for(50), margin_walks_for(100)))
        .unwrap()
        .join()
        .unwrap();
    assert!(at_50 > 0, "the walk was not counted");
    let ratio = at_100 as f64 / at_50 as f64;
    assert!(
        ratio <= 4.5,
        "50 deep: {at_50} calls, 100 deep: {at_100} (×{ratio:.1}; the square of the depth is ×4)"
    );
}
