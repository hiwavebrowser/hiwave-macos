//! Portable pins for FORCED-READ / LAT22: animated property names in the
//! forced-read log are scrubbed (`--*` → `--custom`, known → lowercase,
//! else `other`). Cost fields stay in headless `forced_layout_cause_tests`.

use super::*;

#[test]
fn custom_properties_log_as_custom() {
    assert_eq!(animated_prop_for_log("--fgColor-default"), "--custom");
    assert_eq!(animated_prop_for_log("--theme"), "--custom");
}

#[test]
fn known_animatable_names_are_lowercased() {
    assert_eq!(animated_prop_for_log("Opacity"), "opacity");
    assert_eq!(animated_prop_for_log("TRANSFORM"), "transform");
    assert_eq!(animated_prop_for_log("color"), "color");
}

#[test]
fn unknown_keys_log_as_other() {
    assert_eq!(animated_prop_for_log("not-a-css-property"), "other");
    assert_eq!(animated_prop_for_log("animation-name"), "other");
}
