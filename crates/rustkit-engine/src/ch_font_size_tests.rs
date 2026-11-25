//! CH-REM-FONT-SIZE: `1ch` is the advance of "0" in the element's own font at its computed
//! font-size (CSS Values 3 §5.1.1). `ch_advance_px` read the font-size only when it was written in
//! px and used 16px for every other form, so `font-size: 3.5rem; inline-size: 16ch` laid out at
//! 16 x the advance of a 16px "0". GitHub's home page sets exactly that on its hero `<h1>`
//! (`.Intro__header_title { inline-size: 16ch }`, font-size `var(--brand-text-size-900)` = 3.5rem):
//! once #1442 made `inline-size` apply, the heading shrank to one word per line (149 px wide) and
//! the real-site board read a blank first screen.
//!
//! Oracle: Chrome for Testing 148.0.7778.216 gives the same width for `width: 16ch` and
//! `inline-size: 16ch` at `font-size` 56px, 3.5rem, 3.5em and 350% (498.3 px in sans-serif), so the
//! test pins that every form matches the px form.

use super::*;
use crate::script_net_tests::serve_routes;

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

/// `#b`'s border-box width, for `#b`'s own style.
fn width(own: &str) -> f64 {
    let page = format!(
        "<!doctype html><style>body{{margin:0}}</style><div style=\"width:1158px\">\
         <h1 id=b style=\"margin:0;font-family:sans-serif;{own}\">Software still starts with you</h1></div>"
    );
    let server = serve_routes(vec![("/", "text/html", page)]);
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 1280, height: 800 })
        .expect("view");
    let url = Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap();
    rt().block_on(engine.load_url(view, url)).expect("load_url");
    let value = engine
        .execute_script(view, "String(document.getElementById('b').getBoundingClientRect().width)")
        .unwrap();
    value
        .strip_prefix("String(\"")
        .and_then(|v| v.strip_suffix("\")"))
        .unwrap_or(&value)
        .parse()
        .unwrap_or_else(|_| panic!("width: {value}"))
}

#[test]
fn ch_follows_a_rem_em_or_percent_font_size_like_a_px_one() {
    for property in ["width", "inline-size"] {
        let px = width(&format!("font-size:56px;{property}:16ch"));
        assert!(px > 300.0, "{property}: 16ch at 56px is {px}, the 56px advance was not used");
        for form in ["3.5rem", "3.5em", "350%"] {
            let got = width(&format!("font-size:{form};{property}:16ch"));
            assert!(
                (got - px).abs() < 0.5,
                "{property}: 16ch at font-size {form} is {got} px, at 56px {px} px (Chrome 148: equal)"
            );
        }
    }
}
