//! Text ink: how much coverage a run of glyphs paints, against Chrome on
//! macOS.
//!
//! The chrome board's text read thinner and dimmer than Chromium's on every
//! page: the same glyph footprint, about three quarters of the ink. Chrome's
//! Skia draws macOS text (normal hinting, A8 since `--disable-lcd-text`)
//! from CoreGraphics' SMOOTHED masks, whose outlines are dilated, and runs
//! them through its mask pre-blend for the text colour
//! (`SkScalerContext_Mac::generateImage`, `SkTypeface_Mac::onFilterRec`,
//! Chromium 148). RustKit drew every glyph from the undilated outline, which
//! is what Skia does only for unhinted text: `-webkit-font-smoothing:
//! antialiased` or `text-rendering: geometricPrecision`.
//!
//! The pin is the ink of one line: the sum over the frame of |pixel - page
//! background| over the three channels. Expected values are Chrome for
//! Testing 148.0.7778.216 (the parity baseline), headless with the parity
//! launch flags, on the same page at 240x60. Within 4%: glyph seating is
//! pinned elsewhere, and a pixel's ink moving to its neighbour leaves the
//! total alone.

use super::*;

const WIDTH: u32 = 240;
const HEIGHT: u32 = 60;
const TEXT: &str = "Full automation with undo";

fn ink(color: &str, background: [u8; 3], extra: &str) -> u64 {
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: WIDTH, height: HEIGHT })
        .expect("view");
    let bg = format!("#{:02x}{:02x}{:02x}", background[0], background[1], background[2]);
    let html = format!(
        "<!doctype html><body style=\"margin:0;background:{bg}\"><div style=\"position:absolute;left:2px;top:0;font:13px Arial;color:{color};line-height:24px;white-space:nowrap;{extra}\">{TEXT}</div></body>"
    );
    engine.load_html(view, &html).expect("load_html");
    engine.render_view(view).expect("render");
    let path = std::env::temp_dir().join(format!(
        "rustkit-text-coverage-{}-{:?}.ppm",
        std::process::id(),
        std::thread::current().id()
    ));
    engine.capture_frame(view, path.to_str().unwrap()).expect("capture");
    let ppm = std::fs::read(&path).expect("frame");
    let _ = std::fs::remove_file(&path);
    let px = &ppm[ppm.len() - WIDTH as usize * HEIGHT as usize * 3..];
    px.chunks_exact(3)
        .map(|p| (0..3).map(|c| (p[c] as i32 - background[c] as i32).unsigned_abs() as u64).sum::<u64>())
        .sum()
}

fn assert_ink(color: &str, background: [u8; 3], extra: &str, chrome: u64) {
    let got = ink(color, background, extra);
    let ratio = got as f64 / chrome as f64;
    eprintln!("text ink {color} {extra}: {got} vs Chrome 148 {chrome} ({ratio:.3}x)");
    assert!(
        (0.96..=1.04).contains(&ratio),
        "{color} on {background:?} {extra}: ink {got}, Chrome 148 {chrome} ({ratio:.3}x)"
    );
}

/// Black on white: Chrome 148 ink 338325 (RustKit drew ~0.90x).
#[test]
fn black_text_on_white_carries_chromes_ink() {
    assert_ink("#000", [255, 255, 255], "", 338_325);
}

/// The board's muted sidebar text, #94a3b8 on #1e293b: Chrome 148 ink
/// 187897 (RustKit ~0.77x).
#[test]
fn muted_text_on_dark_carries_chromes_ink() {
    assert_ink("#94a3b8", [0x1e, 0x29, 0x3b], "", 187_897);
}

/// White on #111: Chrome 148 ink 392073 (RustKit ~0.72x).
#[test]
fn white_text_on_dark_carries_chromes_ink() {
    assert_ink("#fff", [17, 17, 17], "", 392_073);
}

/// `-webkit-font-smoothing: antialiased` turns Chrome's dilation off:
/// Chrome 148 ink 304446, the undilated outline (RustKit already ~1.00x).
#[test]
fn antialiased_text_keeps_the_undilated_outline() {
    assert_ink("#000", [255, 255, 255], "-webkit-font-smoothing:antialiased", 304_446);
}

/// The property inherits (`html { -webkit-font-smoothing: antialiased }` is
/// the common site-wide form): the same 304446.
#[test]
fn antialiased_inherits_from_the_root() {
    let got = {
        let mut engine = Engine::new(EngineConfig::default()).expect("engine");
        let view = engine
            .create_headless_view(Bounds { x: 0, y: 0, width: WIDTH, height: HEIGHT })
            .expect("view");
        let html = format!(
            "<!doctype html><html style=\"-webkit-font-smoothing:antialiased\"><body style=\"margin:0;background:#fff\"><div style=\"position:absolute;left:2px;top:0;font:13px Arial;color:#000;line-height:24px;white-space:nowrap\">{TEXT}</div></body></html>"
        );
        engine.load_html(view, &html).expect("load_html");
        engine.render_view(view).expect("render");
        let path = std::env::temp_dir().join(format!(
            "rustkit-text-coverage-inh-{}-{:?}.ppm",
            std::process::id(),
            std::thread::current().id()
        ));
        engine.capture_frame(view, path.to_str().unwrap()).expect("capture");
        let ppm = std::fs::read(&path).expect("frame");
        let _ = std::fs::remove_file(&path);
        let px = &ppm[ppm.len() - WIDTH as usize * HEIGHT as usize * 3..];
        px.chunks_exact(3)
            .map(|p| p.iter().map(|c| (255 - *c) as u64).sum::<u64>())
            .sum::<u64>()
    };
    let ratio = got as f64 / 304_446.0;
    assert!((0.96..=1.04).contains(&ratio), "inherited antialiased: ink {got} ({ratio:.3}x of Chrome 148)");
}


const AHEM: &[u8] = include_bytes!("../../rustkit-text/tests/fixtures/Ahem.ttf");

/// (solid pixels, partially covered pixels, ink) of a 20px Ahem "X" (a
/// full em square) at (10, 10) on white, after the web font has loaded.
fn ahem_square(extra: &str) -> (usize, usize, u64) {
    use base64::Engine as _;
    let uri = format!(
        "data:font/ttf;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(AHEM)
    );
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: WIDTH, height: HEIGHT })
        .expect("view");
    let html = format!(
        "<!doctype html><style>@font-face{{font-family:Ahem;src:url({uri})}}</style><body style=\"margin:0;background:#fff\"><div style=\"position:absolute;left:10px;top:10px;font:20px/20px Ahem;color:#000;{extra}\">X</div></body>"
    );
    engine.load_html(view, &html).expect("load_html");
    engine.render_view(view).expect("render");
    let path = std::env::temp_dir().join(format!(
        "rustkit-text-coverage-ahem-{}-{:?}.ppm",
        std::process::id(),
        std::thread::current().id()
    ));
    engine.capture_frame(view, path.to_str().unwrap()).expect("capture");
    let ppm = std::fs::read(&path).expect("frame");
    let _ = std::fs::remove_file(&path);
    let px = &ppm[ppm.len() - WIDTH as usize * HEIGHT as usize * 3..];
    let cov: Vec<u8> = px.chunks_exact(3).map(|p| 255 - p[1]).collect();
    let solid = cov.iter().filter(|c| **c == 255).count();
    let partial = cov.iter().filter(|c| **c > 0 && **c < 255).count();
    let ink = cov.iter().map(|c| *c as u64).sum::<u64>();
    (solid, partial, ink)
}

/// Chrome 148 dilates Ahem too: 400 solid pixels and a light fringe of 62
/// partial ones (row above at 53/255, right column at 26/255), ink 104105.
/// The undilated square (develop) has the 400 and no fringe.
#[test]
fn an_ahem_square_carries_chromes_light_fringe() {
    let (solid, partial, ink) = ahem_square("");
    eprintln!("ahem default: solid {solid} partial {partial} ink {ink} (Chrome 148: 400, 62, 104105)");
    assert_eq!(solid, 400, "the em square itself");
    assert!((50..=75).contains(&partial), "fringe pixels {partial}, Chrome 148 62");
    let ratio = ink as f64 / 104_105.0;
    assert!((0.98..=1.02).contains(&ratio), "ink {ink} ({ratio:.3}x of Chrome 148)");
}

/// `antialiased`: Chrome 148 draws the hard square, 400 solid, 0 partial.
#[test]
fn an_antialiased_ahem_square_has_no_fringe() {
    let (solid, partial, _) = ahem_square("-webkit-font-smoothing:antialiased");
    assert_eq!((solid, partial), (400, 0));
}
