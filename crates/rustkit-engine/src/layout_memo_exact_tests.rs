//! MEMO-BIT-EXACT: what the layout memo restores is a fresh layout, to within f32 rounding: every
//! number of the layout within 8 ulps of a fresh layout's, and every pixel of the frame within one
//! level per channel (on legion5, card-grid's few-ulp differences move anti-aliasing coverage by
//! one level on 782 pixels, rows 512-522 and others; two memo-off captures are byte-identical, so
//! the capture itself is deterministic).
//!
//! Why ulps and not equality: the layout builds each position left to right from the absolute
//! origin, `(((origin + cursor) + margin) + border) + padding`, rounding at the absolute scale after
//! each term. The memo stores a subtree relative to its box's content origin and adds the new
//! origin back, so a subtree restored at another origin (grid and flex lay an item out at one
//! place and then at another) can round a few ulps differently: websuite card-grid has 161 such
//! fields (143 y, 18 heights summed from restored children), at most 4 ulps (6e-5 px). Making
//! that exact means re-associating every position site (parked row LAYOUT-REASSOCIATE). The bound
//! is twice the largest seen, so it catches a restore that is wrong, not one that rounds.

use super::*;
use rustkit_layout::layout_memo::{set_memo_min_subtree, set_memo_mode, MemoMode};

/// How far apart two f32 values are, in representable steps.
fn ulps(a: f32, b: f32) -> u32 {
    if a == b {
        return 0;
    }
    if a.is_nan() || b.is_nan() || a.signum() != b.signum() {
        return u32::MAX;
    }
    (a.to_bits() as i64 - b.to_bits() as i64).unsigned_abs() as u32
}

/// The layout dump (Debug prints each f32 exactly) and the frame of `page` after its load and one
/// script-triggered relayout, with the memo in `mode`.
fn lay_out(page: &str, mode: MemoMode) -> (String, Vec<u8>) {
    set_memo_mode(Some(mode));
    set_memo_min_subtree(Some(1));
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine.create_headless_view(Bounds { x: 0, y: 0, width: 1280, height: 800 }).expect("view");
    engine.load_html(view, page).expect("load_html");
    let first = format!("{:?}", engine.views[&view].layout.as_ref().expect("laid out"));
    engine
        .execute_script(view, "document.body.appendChild(document.createElement('div')); document.body.offsetHeight; 0")
        .expect("js");
    let second = format!("{:?}", engine.views[&view].layout.as_ref().expect("laid out"));
    let path = std::env::temp_dir().join(format!("memo_exact_{}_{:?}.ppm", std::process::id(), mode));
    engine.capture_frame(view, path.to_str().unwrap()).expect("capture");
    let frame = std::fs::read(&path).expect("frame");
    let _ = std::fs::remove_file(&path);
    set_memo_mode(None);
    set_memo_min_subtree(None);
    (format!("{first}\n----\n{second}"), frame)
}

/// The dumps split into their text and their numbers: the text must match exactly, each number
/// within `max_ulps`. Returns the largest distance and how many numbers differed.
fn compare(off: &str, on: &str, max_ulps: u32) -> Result<(u32, usize), String> {
    let number = |c: char| c.is_ascii_digit() || c == '.' || c == '-' || c == 'e';
    fn split(s: &str, number: &dyn Fn(char) -> bool) -> Vec<(bool, String)> {
        let mut out: Vec<(bool, String)> = Vec::new();
        for c in s.chars() {
            let n = number(c);
            match out.last_mut() {
                Some((k, t)) if *k == n => t.push(c),
                _ => out.push((n, c.to_string())),
            }
        }
        out
    }
    let (a, b) = (split(off, &number), split(on, &number));
    if a.len() != b.len() {
        return Err(format!("different shapes: {} vs {} tokens", a.len(), b.len()));
    }
    let (mut worst, mut differing) = (0u32, 0usize);
    for ((num_a, ta), (_, tb)) in a.iter().zip(&b) {
        if ta == tb {
            continue;
        }
        let parsed = (ta.parse::<f32>(), tb.parse::<f32>());
        match (num_a, parsed) {
            (true, (Ok(x), Ok(y))) => {
                let d = ulps(x, y);
                differing += 1;
                worst = worst.max(d);
                if d > max_ulps {
                    return Err(format!("{ta} vs {tb}: {d} ulps (bound {max_ulps})"));
                }
            }
            _ => return Err(format!("text differs: {ta:?} vs {tb:?}")),
        }
    }
    Ok((worst, differing))
}

#[test]
fn a_restored_layout_matches_a_fresh_one_within_8_ulps_and_one_pixel_level_on_card_grid() {
    let page = include_str!("../../../websuite/cases/card-grid/index.html");
    let ((off, frame_off), (on, frame_on)) = (lay_out(page, MemoMode::Off), lay_out(page, MemoMode::On));
    let (worst, differing) = compare(&off, &on, 8).unwrap_or_else(|e| panic!("memo on vs a fresh layout: {e}"));
    let (pixels, level) = frame_difference(&frame_off, &frame_on);
    eprintln!("card-grid: {differing} numbers differ, at most {worst} ulps; {pixels} pixels differ, at most {level} level");
    assert!(level <= 1, "the memo changed the frame by more than rounding: {pixels} pixels, up to {level} levels");
}

/// Pixels that differ between two captures of the same size, and the largest channel difference.
fn frame_difference(a: &[u8], b: &[u8]) -> (usize, u8) {
    assert_eq!(a.len(), b.len(), "frames of different sizes");
    a.chunks(3).zip(b.chunks(3)).filter(|(p, q)| p != q).fold((0, 0), |(n, m), (p, q)| {
        (n + 1, m.max(p.iter().zip(q).map(|(x, y)| x.abs_diff(*y)).max().unwrap_or(0)))
    })
}

/// The bound has teeth: a restore 1/1000 px off is far outside it.
#[test]
fn the_ulp_comparison_catches_a_restore_that_is_wrong_not_rounded() {
    assert_eq!(compare("y: 922.8", "y: 922.80005", 8), Ok((1, 1)));
    assert!(compare("y: 922.8", "y: 922.801", 8).is_err());
    assert!(compare("Block", "Inline", 8).is_err());
}

