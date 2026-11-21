//! MEDIA-AVF M1.1: a replaced box whose key starts with `NO_RATIO_KEY_PREFIX`
//! (`<video>` / `<audio controls>` before metadata) has natural sizes and no
//! natural ratio. Sole prior pin is headless `media_box_tests`; Linux default
//! `--lib` never saw the sizing rules. Chrome for Testing 148.0.7778.216
//! receipt `docs/diagnostics/2026-11-18/media_box_cft148.json`.

use super::*;

fn video(edit: impl FnOnce(&mut ComputedStyle)) -> LayoutBox {
    let mut s = ComputedStyle::new();
    edit(&mut s);
    LayoutBox::new(
        BoxType::Image {
            url: format!("{NO_RATIO_KEY_PREFIX}video:1"),
            natural_width: 300.0,
            natural_height: 150.0,
        },
        s,
    )
}

fn laid(box_: LayoutBox) -> (f32, f32) {
    let mut body = LayoutBox::new(BoxType::Block, ComputedStyle::new());
    body.children.push(box_);
    let cb = Dimensions {
        content: Rect::new(0.0, 0.0, 800.0, 0.0),
        ..Default::default()
    };
    let mut mc = MarginCollapseContext::new();
    let mut fc = FloatContext::new();
    body.layout_with_collapse(&cb, &mut mc, &mut fc);
    let c = body.children[0].dimensions.content;
    (c.width, c.height)
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.5
}

#[test]
fn key_has_no_ratio_matches_the_prefix_only() {
    assert!(key_has_no_ratio(&format!("{NO_RATIO_KEY_PREFIX}video:1")));
    assert!(key_has_no_ratio(&format!("{NO_RATIO_KEY_PREFIX}audio:2")));
    assert!(!key_has_no_ratio("https://example.test/v.mp4"));
    assert!(!key_has_no_ratio("rustkit-canvas:1"));
}

#[test]
fn an_unsized_video_takes_its_natural_300_by_150() {
    let (w, h) = laid(video(|_| {}));
    assert!(close(w, 300.0) && close(h, 150.0), "{w}x{h}, Chrome 300x150");
}

#[test]
fn a_width_alone_keeps_the_natural_height() {
    // Chrome: `<video width=100>` → 100x150 (no ratio transfer).
    let (w, h) = laid(video(|s| s.width = Length::Px(100.0)));
    assert!(close(w, 100.0) && close(h, 150.0), "{w}x{h}, Chrome 100x150");
}

#[test]
fn a_height_alone_keeps_the_natural_width() {
    let (w, h) = laid(video(|s| s.height = Length::Px(60.0)));
    assert!(close(w, 300.0) && close(h, 60.0), "{w}x{h}, Chrome 300x60");
}

#[test]
fn a_max_height_does_not_scale_the_width() {
    // With a natural ratio, max-height 75 on 300x150 would shrink width to 150.
    // No-ratio keeps width at 300 and only clamps height.
    let (w, h) = laid(video(|s| s.max_height = Length::Px(75.0)));
    assert!(
        close(w, 300.0) && close(h, 75.0),
        "{w}x{h}, want 300x75 (no cross-axis scale)"
    );
}
