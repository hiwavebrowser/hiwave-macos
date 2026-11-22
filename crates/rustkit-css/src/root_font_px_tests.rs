//! ROOT-REM (#1408): portable pins for the layout rem TLS.
//!
//! `root_rem_tests` / `root_rem_cross_view_tests` are headless (and the former
//! macos-only). Default Linux `--lib` never saw `set_root_font_px` clamp, nor
//! that `TransformList::to_matrix` resolves `rem` against that TLS instead of
//! the initial 16px. Do not confuse with CSSOM `computed_read::set_root_font`.

use crate::{set_root_font_px, root_font_px, Length, TransformList, TransformOp};

struct RestoreRoot(f32);
impl Drop for RestoreRoot {
    fn drop(&mut self) {
        let _ = set_root_font_px(self.0);
    }
}

fn with_root(px: f32) -> RestoreRoot {
    RestoreRoot(set_root_font_px(px))
}

#[test]
fn the_root_font_defaults_to_sixteen_after_a_reset() {
    let _guard = with_root(16.0);
    assert_eq!(root_font_px(), 16.0);
}

#[test]
fn set_root_font_px_returns_the_previous_and_clamps_bad_values() {
    let _guard = with_root(16.0);
    let prev = set_root_font_px(20.0);
    assert_eq!(prev, 16.0);
    assert_eq!(root_font_px(), 20.0);

    assert_eq!(set_root_font_px(f32::NAN), 20.0);
    assert_eq!(root_font_px(), 16.0);

    let _ = set_root_font_px(24.0);
    assert_eq!(set_root_font_px(-1.0), 24.0);
    assert_eq!(root_font_px(), 16.0);
}

#[test]
fn a_translate_rem_follows_the_root_font_not_sixteen() {
    let _guard = with_root(10.0);
    let list = TransformList {
        ops: vec![TransformOp::Translate(Length::Rem(1.5), Length::Rem(2.0))],
    };
    let m = list.to_matrix(100.0, 100.0);
    assert!((m[4] - 15.0).abs() < 1e-5, "tx: got {}", m[4]);
    assert!((m[5] - 20.0).abs() < 1e-5, "ty: got {}", m[5]);
}
