//! Portable LOGICAL-SIZE pins (#1442 / batch 125).
//!
//! The Chrome oracle in `logical_size_tests` needs `feature = "headless"`, so
//! default Linux `--lib` never saw the declaration map or the inherit remap.
//! These call `apply_style_property` / `apply_cascaded` directly.

use super::*;

fn applied(decls: &[(&str, &str)]) -> ComputedStyle {
    let engine = Engine::new(EngineConfig::default()).expect("engine");
    let mut style = ComputedStyle::default();
    for &(property, value) in decls {
        engine.lctx.apply_style_property(&mut style, property, value);
    }
    style
}

fn inherit_size(parent_decls: &[(&str, &str)], property: &str) -> ComputedStyle {
    let engine = Engine::new(EngineConfig::default()).expect("engine");
    let mut parent = ComputedStyle::default();
    for &(p, v) in parent_decls {
        engine.lctx.apply_style_property(&mut parent, p, v);
    }
    let mut style = ComputedStyle::default();
    // Seed a different size so a no-op inherit is visible.
    engine
        .lctx
        .apply_style_property(&mut style, "width", "10px");
    engine
        .lctx
        .apply_style_property(&mut style, "height", "10px");
    engine
        .lctx
        .apply_cascaded(&mut style, Some(&parent), property, "inherit");
    style
}

#[test]
fn the_six_logical_sizes_apply_as_their_physical_properties() {
    let style = applied(&[
        ("inline-size", "200px"),
        ("block-size", "50px"),
        ("min-inline-size", "80px"),
        ("min-block-size", "40px"),
        ("max-inline-size", "300px"),
        ("max-block-size", "90px"),
    ]);
    assert_eq!(style.width, rustkit_css::Length::Px(200.0));
    assert_eq!(style.height, rustkit_css::Length::Px(50.0));
    assert_eq!(style.min_width, rustkit_css::Length::Px(80.0));
    assert_eq!(style.min_height, rustkit_css::Length::Px(40.0));
    assert_eq!(style.max_width, rustkit_css::Length::Px(300.0));
    assert_eq!(style.max_height, rustkit_css::Length::Px(90.0));
    assert!(style.size_declared[0]);
    assert!(style.size_declared[1]);
}

#[test]
fn a_later_logical_or_physical_size_wins_on_the_same_box() {
    let logical_last = applied(&[("width", "100px"), ("inline-size", "200px")]);
    assert_eq!(logical_last.width, rustkit_css::Length::Px(200.0));

    let physical_last = applied(&[("inline-size", "200px"), ("width", "100px")]);
    assert_eq!(physical_last.width, rustkit_css::Length::Px(100.0));

    let cleared = applied(&[("max-width", "120px"), ("max-inline-size", "none")]);
    assert_eq!(cleared.max_width, rustkit_css::Length::Auto);

    let auto = applied(&[("height", "90px"), ("block-size", "auto")]);
    assert_eq!(auto.height, rustkit_css::Length::Auto);
}

#[test]
fn fit_content_and_percent_logical_sizes_reach_the_physical_fields() {
    let fit = applied(&[("inline-size", "fit-content")]);
    assert_eq!(fit.width, rustkit_css::Length::FitContent);

    let percent = applied(&[("inline-size", "50%"), ("block-size", "20%")]);
    assert_eq!(percent.width, rustkit_css::Length::Percent(50.0));
    assert_eq!(percent.height, rustkit_css::Length::Percent(20.0));
}

#[test]
fn logical_size_inherit_copies_the_physical_parent_value() {
    let width = inherit_size(&[("width", "300px")], "inline-size");
    assert_eq!(width.width, rustkit_css::Length::Px(300.0));
    assert_eq!(width.height, rustkit_css::Length::Px(10.0));

    let height = inherit_size(&[("height", "250px")], "block-size");
    assert_eq!(height.height, rustkit_css::Length::Px(250.0));
    assert_eq!(height.width, rustkit_css::Length::Px(10.0));

    let max = inherit_size(&[("max-width", "180px")], "max-inline-size");
    assert_eq!(max.max_width, rustkit_css::Length::Px(180.0));
}
