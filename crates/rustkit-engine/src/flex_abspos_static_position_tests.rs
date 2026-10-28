//! The static position of an absolutely positioned child of a flex
//! container (CSS Flexbox 1 section 4.1): it is placed as if it were the
//! sole flex item, so `align-items` and `justify-content` apply, and a box
//! taller than the content box overflows on the start side under
//! `align-items: flex-end`.
//!
//! netflix.com's logged-out hero is a `position: relative` flex row with
//! `align-items: flex-end` and `padding-top: 100px`; its background layer
//! is `position: absolute; height: 100%; width: 100%` with no offsets.
//! Chrome puts the layer at the top of the hero (y 0); RustKit pins it to
//! the content box (y 100 here, under the hero on the live page), so the
//! hero image is missing from the first screen (Pete's hand test 10-27).
//!
//! The expected rows are what the pinned Chrome 148 reports for this page:
//! `id:x:y:width:height` of each border box. `col` is a control that
//! already matches.

use super::*;

const PAGE: &str = r#"<!doctype html>
<html><head><style>
body { margin: 0 }
* { box-sizing: border-box }
.hero { position: relative; display: flex; align-items: flex-end; justify-content: center; padding-top: 100px; overflow: hidden; width: 1000px; }
.content { width: 300px; height: 500px; }
.bg { position: absolute; height: 100%; width: 100%; overflow: hidden; }
.img { position: absolute; width: 100%; height: 100%; }
.col { position: relative; display: flex; flex-direction: column; justify-content: center; align-items: center; width: 400px; height: 300px; }
.small { position: absolute; width: 50px; height: 40px; }
.c2 { width: 100px; height: 100px; }
</style></head><body>
<div class="hero" id="hero"><div class="content" id="content"></div><div class="bg" id="bg"><div class="img" id="img"></div></div></div>
<div class="hero" id="hero2"><div class="bg" id="bg2"><div class="img" id="img2"></div></div><div class="content" id="content2"></div></div>
<div class="col" id="col"><div class="c2" id="c2"></div><div class="small" id="small"></div></div>
</body></html>"#;

const CHROME: &str = "hero:0:0:1000:600 content:350:100:300:500 bg:0:0:1000:600 img:0:0:1000:600 \
    hero2:0:600:1000:600 bg2:0:600:1000:600 img2:0:600:1000:600 content2:350:700:300:500 \
    col:0:1200:400:300 c2:150:1300:100:100 small:175:1330:50:40";

const BOXES: &str = "Array.prototype.map.call(document.querySelectorAll('[id]'), function (e) {\
    var r = e.getBoundingClientRect();\
    return e.id + ':' + Math.round(r.left) + ':' + Math.round(r.top) + ':' + Math.round(r.width) + ':' + Math.round(r.height);\
    }).join(' ')";

#[test]
#[cfg(all(target_os = "macos", feature = "headless"))]
fn an_abspos_flex_child_is_placed_as_the_sole_flex_item() {
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let id = engine
        .create_headless_view(Bounds::new(0, 0, 1280, 800))
        .expect("headless view");
    engine.load_html(id, PAGE).expect("load_html");
    let value = engine.execute_script(id, BOXES).expect("script");
    let got = value
        .strip_prefix("String(\"")
        .and_then(|v| v.strip_suffix("\")"))
        .unwrap_or(&value);
    assert_eq!(got, CHROME.split_whitespace().collect::<Vec<_>>().join(" "));
}
