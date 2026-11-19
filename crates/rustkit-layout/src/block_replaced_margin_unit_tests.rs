//! Block-level replaced boxes (`display: block` on `BoxType::Image`) collapse
//! margins as blocks do. Sole prior pin is headless `block_replaced_margin_tests`
//! (Chrome for Testing 148.0.7778.216 receipt
//! `docs/diagnostics/2026-11-19/block_replaced_margin_cft148.json`). These
//! layout-only cases keep the collapse contract on Linux default `--lib`.

use super::*;
use rustkit_css::Display;

fn block(h: f32, edit: impl FnOnce(&mut ComputedStyle)) -> LayoutBox {
    let mut s = ComputedStyle::new();
    s.display = Display::Block;
    s.height = Length::Px(h);
    edit(&mut s);
    LayoutBox::new(BoxType::Block, s)
}

fn img(natural: f32, edit: impl FnOnce(&mut ComputedStyle)) -> LayoutBox {
    let mut s = ComputedStyle::new();
    s.display = Display::Block;
    edit(&mut s);
    LayoutBox::new(
        BoxType::Image {
            url: String::new(),
            natural_width: natural,
            natural_height: natural,
        },
        s,
    )
}

fn laid(children: Vec<LayoutBox>) -> LayoutBox {
    let mut body = LayoutBox::new(BoxType::Block, ComputedStyle::new());
    body.style.display = Display::Block;
    body.children = children;
    let cb = Dimensions {
        content: Rect::new(0.0, 0.0, 800.0, 0.0),
        ..Default::default()
    };
    let mut mc = MarginCollapseContext::new();
    let mut fc = FloatContext::new();
    body.layout_with_collapse(&cb, &mut mc, &mut fc);
    body
}

fn y_h(b: &LayoutBox) -> (f32, f32) {
    (b.dimensions.content.y, b.dimensions.content.height)
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.5
}

#[test]
fn a_block_img_collapses_with_the_margins_around_it() {
    // Chrome: a=0/20 e=50/10 n=72/10 (max(30,12)=30 after a).
    let body = laid(vec![
        block(20.0, |s| s.margin_bottom = Length::Px(30.0)),
        img(10.0, |s| {
            s.margin_top = Length::Px(12.0);
            s.margin_bottom = Length::Px(12.0);
        }),
        block(10.0, |_| {}),
    ]);
    let (ay, ah) = y_h(&body.children[0]);
    let (ey, eh) = y_h(&body.children[1]);
    let (ny, nh) = y_h(&body.children[2]);
    assert!(close(ay, 0.0) && close(ah, 20.0), "a={ay}/{ah}");
    assert!(close(ey, 50.0) && close(eh, 10.0), "e={ey}/{eh}, Chrome 50/10");
    assert!(close(ny, 72.0) && close(nh, 10.0), "n={ny}/{nh}, Chrome 72/10");
}

#[test]
fn a_zero_height_block_img_is_not_collapsed_through() {
    // Chrome: a=0/20 e=50/0 n=62/10 — margins never collapse through the image.
    let body = laid(vec![
        block(20.0, |s| s.margin_bottom = Length::Px(30.0)),
        img(10.0, |s| {
            s.height = Length::Px(0.0);
            s.margin_top = Length::Px(12.0);
            s.margin_bottom = Length::Px(12.0);
        }),
        block(10.0, |_| {}),
    ]);
    let (ey, eh) = y_h(&body.children[1]);
    let (ny, _) = y_h(&body.children[2]);
    assert!(close(ey, 50.0) && close(eh, 0.0), "e={ey}/{eh}");
    assert!(close(ny, 62.0), "n.y={ny}, Chrome 62");
}

#[test]
fn two_block_imgs_collapse_with_each_other() {
    // Chrome: e=0/10 f=35/10 (max(15,25)=25 between them).
    let body = laid(vec![
        img(10.0, |s| s.margin_bottom = Length::Px(15.0)),
        img(10.0, |s| s.margin_top = Length::Px(25.0)),
    ]);
    let (ey, eh) = y_h(&body.children[0]);
    let (fy, fh) = y_h(&body.children[1]);
    assert!(close(ey, 0.0) && close(eh, 10.0), "e={ey}/{eh}");
    assert!(close(fy, 35.0) && close(fh, 10.0), "f={fy}/{fh}, Chrome 35/10");
}

#[test]
fn an_inline_img_materialises_the_pending_margin_instead_of_collapsing() {
    // Same siblings as the block case, but the image stays `display: inline`.
    // The pending 30px is resolved into the cursor before the line box, so the
    // next block starts at 20+30+line — not at the collapsed 50/72 geometry.
    let body = laid(vec![
        block(20.0, |s| s.margin_bottom = Length::Px(30.0)),
        img(10.0, |s| {
            s.display = Display::Inline;
            s.margin_top = Length::Px(12.0);
            s.margin_bottom = Length::Px(12.0);
        }),
        block(10.0, |_| {}),
    ]);
    let (ey, _) = y_h(&body.children[1]);
    let (ny, _) = y_h(&body.children[2]);
    assert!(
        !close(ey, 50.0),
        "inline e.y={ey}: must not take the block-collapse y of 50"
    );
    assert!(
        !close(ny, 72.0),
        "n.y={ny}: must not match the block-collapse companion (72)"
    );
}
