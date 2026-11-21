//! REG-TRANS-FASTPATH residual: gradient layers that stay gradient layers, with
//! the same position/size/repeat/origin/clip, are paint (`copy_paint_only` takes
//! their stops). A URL layer change, or a gradient whose geometry moved, is not.
//!
//! Open #1267's `paint_only_diff_tests` pins colour/width/transform basics only.
//! These cases exercise `gradient_backgrounds_alike` on default `--lib`.

use super::*;

fn stop(r: u8, g: u8, b: u8) -> ColorStop {
    ColorStop::new(Color::from_rgb(r, g, b), Some(0.0))
}

fn linear(stops: Vec<ColorStop>) -> Gradient {
    Gradient::Linear(LinearGradient::new(GradientDirection::ToBottom, stops))
}

fn gradient_style(stops: Vec<ColorStop>) -> ComputedStyle {
    let mut s = ComputedStyle::default();
    s.background_layers = vec![BackgroundLayer::from_gradient(linear(stops))];
    s
}

#[test]
fn a_gradient_stop_change_differs_only_in_paint() {
    let a = gradient_style(vec![stop(0, 0, 255), stop(255, 255, 255)]);
    let b = gradient_style(vec![stop(255, 0, 0), stop(255, 255, 255)]);
    assert!(!a.same_paint(&b), "stops differ so same_paint is false");
    assert!(
        a.differs_only_in_paint(&b),
        "gradient stop changes are paint-only on the short path"
    );

    let mut dest = a.clone();
    dest.copy_paint_only(&b);
    assert_eq!(dest.background_layers, b.background_layers);
}

#[test]
fn a_url_layer_change_is_not_paint_only() {
    let mut a = ComputedStyle::default();
    a.background_layers = vec![BackgroundLayer::from_url("a.png".into())];
    let mut b = a.clone();
    b.background_layers = vec![BackgroundLayer::from_url("b.png".into())];
    assert!(!a.same_paint(&b));
    assert!(
        !a.differs_only_in_paint(&b),
        "URL image changes must miss the paint short path"
    );

    let mut dest = a.clone();
    dest.copy_paint_only(&b);
    assert_eq!(
        dest.background_layers, a.background_layers,
        "copy_paint_only must leave a URL layer alone"
    );
}

#[test]
fn a_gradient_position_change_is_not_paint_only() {
    let a = gradient_style(vec![stop(0, 0, 0), stop(255, 255, 255)]);
    let mut b = a.clone();
    b.background_layers[0].position = BackgroundPosition {
        x: BackgroundPositionValue::Percent(0.5),
        y: BackgroundPositionValue::Percent(0.5),
    };
    assert!(!a.same_paint(&b));
    assert!(
        !a.differs_only_in_paint(&b),
        "geometry of a gradient layer is not paint-only"
    );

    let mut dest = a.clone();
    dest.copy_paint_only(&b);
    assert_eq!(
        dest.background_layers, a.background_layers,
        "copy_paint_only must not take a moved gradient layer"
    );
}
