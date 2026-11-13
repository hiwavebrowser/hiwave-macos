//! Canvas 2D pixels (STR-7): each script in
//! tools/parity_oracle/canvas_raster_cases.json draws on a fresh 40x40
//! canvas and `getImageData` reads it back; the pixels are compared with
//! Chrome for Testing 148's (canvas_raster_log.mjs).
//!
//! Chrome rasterizes with Skia and the engine with tiny-skia, a port of
//! Skia's CPU pipeline, so covered and uncovered pixels match exactly
//! and only anti-aliased edge pixels may differ, by a few levels of
//! coverage. A pixel is "off" when a channel differs by more than
//! [`EDGE`], "wrong" when one differs by more than [`WRONG`]: no case may
//! have a wrong pixel, and off pixels stay within [`OFF_SHARE`] of the
//! pixels Chrome painted.

use super::*;

/// Channel difference an anti-aliased edge pixel may show.
const EDGE: u8 = 2;
/// Channel difference no pixel may show (a wrong colour, a missed or
/// extra pixel).
const WRONG: u8 = 40;
/// Share of the painted pixels that may be more than `EDGE` off.
const OFF_SHARE: f32 = 0.15;

pub(crate) struct Compared {
    pub(crate) wrong: usize,
    pub(crate) off: usize,
    pub(crate) painted: usize,
    pub(crate) max: u8,
}

pub(crate) fn compare(ours: &[u8], chrome: &[u8]) -> Compared {
    let mut c = Compared { wrong: 0, off: 0, painted: 0, max: 0 };
    for (a, b) in ours.chunks(4).zip(chrome.chunks(4)) {
        if b[3] != 0 {
            c.painted += 1;
        }
        let d = a.iter().zip(b).map(|(x, y)| x.abs_diff(*y)).max().unwrap_or(0);
        c.max = c.max.max(d);
        if d > WRONG {
            c.wrong += 1;
        } else if d > EDGE {
            c.off += 1;
        }
    }
    c
}

pub(crate) fn unhex(hex: &str) -> Vec<u8> {
    (0..hex.len() / 2).map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap()).collect()
}

/// Runs `script` (the context is `x`) on a fresh `w`x`h` canvas and reads
/// it back with `getImageData`, as hex RGBA.
pub(crate) fn draw(script: &str, w: u32, h: u32) -> String {
    let bindings = DomBindings::new(JsRuntime::new().unwrap()).unwrap();
    bindings
        .set_document(Rc::new(Document::parse_html("<!DOCTYPE html><html><body></body></html>").unwrap()))
        .unwrap();
    let js = format!(
        "(function () {{ var cv = document.createElement('canvas'); cv.width = {w}; cv.height = {h}; \
         document.body.appendChild(cv); var x = cv.getContext('2d'); \
         (function (x) {{ {script} }})(x); \
         var d = x.getImageData(0, 0, {w}, {h}).data, hex = ''; \
         for (var i = 0; i < d.length; i++) hex += (d[i] < 16 ? '0' : '') + d[i].toString(16); \
         return hex; }})()"
    );
    match bindings.evaluate(&js) {
        Ok(JsValue::String(s)) => s,
        other => panic!("{script} evaluated to {other:?}"),
    }
}

pub(crate) fn check_file(json: &str) {
    let data: serde_json::Value = serde_json::from_str(json).unwrap();
    let w = data["size"][0].as_u64().unwrap() as u32;
    let h = data["size"][1].as_u64().unwrap() as u32;
    let mut failures = Vec::new();
    for case in data["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let chrome = unhex(case["chrome_rgba"].as_str().unwrap());
        let ours = unhex(&draw(case["script"].as_str().unwrap(), w, h));
        assert_eq!(ours.len(), chrome.len(), "{name}: getImageData length");
        let c = compare(&ours, &chrome);
        let allowed = (c.painted as f32 * OFF_SHARE).ceil() as usize;
        eprintln!("{name}: painted {} off {} wrong {} max {}", c.painted, c.off, c.wrong, c.max);
        if c.wrong > 0 || c.off > allowed {
            failures.push(format!("{name}: {} wrong, {} off (allowed {allowed}), max diff {}", c.wrong, c.off, c.max));
        }
    }
    assert!(failures.is_empty(), "pixels unlike Chrome 148:\n{}", failures.join("\n"));
}

#[test]
fn canvas_pixels_match_chrome() {
    check_file(include_str!("../../../tools/parity_oracle/canvas_raster_cases.json"));
}
