//! A percentage height on an absolutely positioned replaced element whose
//! containing block is itself an `absolute; inset: 0` layer (its height
//! comes from the insets, not from a `height` declaration).
//!
//! railway.com's hero image is `absolute inset-0 w-full h-full object-cover`
//! inside an `absolute inset-0` layer 1700 px tall: Chrome 148 draws it
//! 1248x1700, RustKit 1248x1248 (the image's aspect ratio wins, so the
//! bottom of the hero has no picture). netflix's hero layer has the same
//! shape. Pete's hand test 10-27. `b` is a control that already matches.
//!
//! Expected row from the pinned Chrome 148.

use super::*;

const PNG: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

const CHROME: &str = "a:0:0:600:400 b:0:410:600:400 c:0:820:600:400";

const BOXES: &str = "Array.prototype.map.call(document.querySelectorAll('img'), function (e) {\
    var r = e.getBoundingClientRect();\
    return e.id + ':' + Math.round(r.left) + ':' + Math.round(r.top) + ':' + Math.round(r.width) + ':' + Math.round(r.height);\
    }).join(' ')";

fn page() -> String {
    format!(
        "<!doctype html><html><head><style>\
         body {{ margin: 0 }}\
         .box {{ position: relative; width: 600px; height: 400px; margin-bottom: 10px }}\
         .layer {{ position: absolute; inset: 0 }}\
         img {{ display: block }}\
         .a {{ position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover }}\
         .b {{ position: absolute; width: 100%; height: 100% }}\
         .c {{ position: absolute; top: 0; left: 0; width: 100%; height: 100% }}\
         </style></head><body>\
         <div class=box><div class=layer><img id=a class=a src='{PNG}'></div></div>\
         <div class=box><img id=b class=b src='{PNG}'></div>\
         <div class=box><div class=layer><img id=c class=c src='{PNG}'></div></div>\
         </body></html>"
    )
}

#[test]
#[cfg(all(target_os = "macos", feature = "headless"))]
fn a_percent_height_image_in_an_inset_sized_layer_fills_the_layer() {
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let id = engine
        .create_headless_view(Bounds::new(0, 0, 1280, 1300))
        .expect("headless view");
    engine.load_html(id, &page()).expect("load_html");
    let value = engine.execute_script(id, BOXES).expect("script");
    let got = value
        .strip_prefix("String(\"")
        .and_then(|v| v.strip_suffix("\")"))
        .unwrap_or(&value);
    assert_eq!(got, CHROME);
}
