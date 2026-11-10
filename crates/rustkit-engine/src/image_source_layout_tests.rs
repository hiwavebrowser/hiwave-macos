//! `<img>` source selection and density-corrected natural size, through the engine (CH2.E54b).
//! The expected values were measured in Chromium 143.0.7499.4 at 800x600
//! (docs/diagnostics/2026-11-10/e54b_srcset_chromium.txt); the images are solid PNG data URLs of
//! 100, 200, 400 and 800 px wide, half as high.

use super::*;

const P100: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAGQAAAAyCAIAAAAlV+npAAAAX0lEQVR42u3QgQAAAAgDsArp/gBhpRDAhrDepPgZBbJkyZIlS5YCWbJkyZIlS4EsWbJkyZKlQJYsWbJkyVIgS5YsWbJkKZAlS5YsWbIUyJIlS5YsWQpkyZIlS5YsBX8HneQBkFZWbZIAAAAASUVORK5CYII=";
const P200: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAMgAAABkCAIAAABM5OhcAAAAyklEQVR42u3SQREAAAQAQUTSP4BYSnh47Ea4uZzugGslAcbCWBgLjIWxMBYYC2NhLDAWxsJYYCyMhbHAWBgLY4GxMBbGAmNhLIwFxsJYGAuMhbEwFhgLY2EsMBbGwlhgLIyFscBYGAtjgbEwFsYCY2EsjAXGwlgYC4yFsTAWGAtjYSwwFsbCWGAsjIWxwFgYC2OBsTAWxgJjYSyMBcbCWBgLjIWxMBYYC2NhLDAWxsJYYCyMhbEwFhgLY2EsMBbGwlhgLIyFscBY/LbQwwH0ss81ugAAAABJRU5ErkJggg==";
const P400: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAZAAAADICAIAAABJdyC1AAACA0lEQVR42u3UMQ0AAAjAMEAS/gUgCxUcJK2EHcvpDoAPSgLAsAAMCzAsAMMCMCzAsAAMC8CwAMMCMCwAwwIMC8CwAAwLMCwAwwIwLMCwAAwLwLAAwwIwLADDAgwLwLAADAswLADDAgwLwLAADAswLADDAjAswLAADAvAsADDAjAsAMMCDAvAsAAMCzAsAMMCMCzAsAAMC8CwAMMCMCwAwwIMC8CwAAwLMCwAwwIMC8CwAAwLMCwAwwIwLMCwAAwLwLAAwwIwLADDAgwLwLAADAswLADDAjAswLAADAvAsADDAjAsAMMCDAvAsAAMCzAsAMMCDAvAsAAMCzAsAMMCMCzAsAAMC8CwAMMCMCwAwwIMC8CwAAwLMCwAwwIwLMCwAAwLwLAAwwIwLADDAgwLwLAADAswLADDAgwLwLAADAswLADDAjAswLAADAvAsADDAjAsAMMCDAvAsAAMCzAsAMMCMCzAsAAMC8CwAMMCMCwAwwIMC8CwAAwLMCwAwwIMC8CwAAwLMCwAwwIwLMCwAAwLwLAAwwIwLADDAgwLwLAADAswLADDAjAswLAADAvAsADDAjAsAMMCDAvAsAAMCzAsAMMCDAvAsAAMCzAsAMMCMCzAsAAMC8CwAMMCMCwAwwIMC8CwAAwLMCwAwwIwLMCwAAwLwLAAwwIwLADDAgwL4M4CqLoCvG9U14EAAAAASUVORK5CYII=";
const P800: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAyAAAAGQCAIAAADZR5NjAAAF6UlEQVR42u3WMQ0AAAjAMEAS/gUgCw0knK2EXcvpDgAA/pQEAAAGCwDAYAEAGCwAAAwWAIDBAgAwWAAAGCwAAIMFAGCwAAAwWAAABgsAwGABABgsAAAMFgCAwQIAMFgAABgsAACDBQBgsAAAMFgAAAYLAMBgAQBgsAAADBYAgMECADBYAAAYLAAAgwUAYLAAADBYAAAGCwDAYAEAYLAAAAwWAIDBAgDAYAEAGCwAAIMFAGCwAAAwWAAABgsAwGABAGCwAAAMFgCAwQIAwGABABgsAACDBQCAwQIAMFgAAAYLAMBgAQBgsAAADBYAgMECAMBgAQAYLAAAgwUAgMECADBYAAAGCwDAYAEAYLAAAAwWAIDBAgDAYAEAGCwAAIMFAIDBAgAwWAAABgsAAIMFAGCwAAAMFgCAwQIAwGABABgsAACDBQCAwQIAMFgAAAYLAACDBQBgsAAADBYAAAYLAMBgAQAYLAAAgwUAgMECADBYAAAGCwAAgwUAYLAAAAwWAAAGCwDAYAEAGCwAAAwWAIDBAgAwWAAABgsAAIMFAGCwAAAMFgAABgsAwGABABgsAAAMFgCAwQIAMFgAAAYLAACDBQBgsAAADBYAAAYLAMBgAQAYLAAADBYAgMECADBYAAAYLAAAgwUAYLAAAAwWAAAGCwDAYAEAGCwAAAwWAIDBAgAwWAAAGCwAAIMFAGCwAAAwWAAABgsAwGABABgsAAAMFgCAwQIAMFgAABgsAACDBQBgsAAAMFgAAAYLAMBgAQBgsAAADBYAgMECADBYAAAYLAAAgwUAYLAAADBYAAAGCwDAYAEAYLAAAAwWAIDBAgAwWAAAGCwAAIMFAGCwAAAwWAAABgsAwGABAGCwAAAMFgCAwQIAwGABABgsAACDBQBgsAAAMFgAAAYLAMBgAQBgsAAADBYAgMECAMBgAQAYLAAAgwUAgMECADBYAAAGCwDAYAEAYLAAAAwWAIDBAgDAYAEAGCwAAIMFAIDBAgAwWAAABgsAAIMFAGCwAAAMFgCAwQIAwGABABgsAACDBQCAwQIAMFgAAAYLAACDBQBgsAAADBYAgMECAMBgAQAYLAAAgwUAgMECADBYAAAGCwAAgwUAYLAAAAwWAAAGCwDAYAEAGCwAAIMFAIDBAgAwWAAABgsAAIMFAGCwAAAMFgAABgsAwGABABgsAAAMFgCAwQIAMFgAAAYLAACDBQBgsAAADBYAAAYLAMBgAQAYLAAADBYAgMECADBYAAAYLAAAgwUAYLAAAAwWAAAGCwDAYAEAGCwAAAwWAIDBAgAwWAAAGCwAAIMFAGCwAAAMlgQAAAYLAMBgAQAYLAAADBYAgMECADBYAAAYLAAAgwUAYLAAADBYAAAGCwDAYAEAGCwAAAwWAIDBAgAwWAAAGCwAAIMFAGCwAAAwWAAABgsAwGABAGCwAAAMFgCAwQIAMFgAABgsAACDBQBgsAAAMFgAAAYLAMBgAQBgsAAADBYAgMECAMBgAQAYLAAAgwUAYLAAADBYAAAGCwDAYAEAYLAAAAwWAIDBAgDAYAEAGCwAAIMFAIDBAgAwWAAABgsAwGABAGCwAAAMFgCAwQIAwGABABgsAACDBQCAwQIAMFgAAAYLAMBgAQBgsAAADBYAgMECAMBgAQAYLAAAgwUAgMECADBYAAAGCwAAgwUAYLAAAAwWAIDBAgDAYAEAGCwAAIMFAIDBAgAwWAAABgsAAIMFAGCwAAAMFgAABgsAwGABABgsAACDBQCAwQIAMFgAAAYLAACDBQBgsAAADBYAAAYLAMBgAQAYLAAADBYAgMECADBYAAAGCwAAgwUAYLAAAAwWAAAGCwDAYAEAGCwAAAwWAIDBAgAwWAAABgsAAIMFAGCwAAAMFgAABgsAwGABABgsAAAMFgCAwQIAMFgAABgsAACDBQBgsAAADBYAAAYLAMBgAQAYLAAADBYAgMECADBYAAAYLAAAgwUAYLAAADBYAAAGCwDAYAEAGCwAAAwWAIDBAgAwWAAAXCwtmwRM2TIY/AAAAABJRU5ErkJggg==";

fn engine(dpr: Option<f32>) -> (Engine, EngineViewId) {
    let mut engine = Engine::new(EngineConfig { device_pixel_ratio: dpr, ..EngineConfig::default() }).expect("engine");
    let view = engine.create_headless_view(Bounds { x: 0, y: 0, width: 800, height: 600 }).expect("view");
    (engine, view)
}

/// The shown size of `#i` after layout, as `WxH`.
fn shown(dpr: Option<f32>, img: &str) -> String {
    let (mut engine, view) = engine(dpr);
    engine
        .load_html(view, &format!("<!doctype html><body style=\"margin:0\">{img}</body>"))
        .expect("load_html");
    let got = engine
        .execute_script(view, "var r = document.getElementById('i').getBoundingClientRect(); r.width + 'x' + r.height")
        .expect("script");
    got.strip_prefix("String(\"").and_then(|v| v.strip_suffix("\")")).unwrap_or(&got).to_string()
}

#[test]
fn a_2x_candidate_is_shown_at_half_its_pixel_width() {
    // Chrome: <img srcset="b.png 2x"> (b.png is 200 px) is 100x50 at ratio 1 and at ratio 2.
    let img = format!("<img id=i srcset=\"{P200} 2x\">");
    assert_eq!(shown(Some(1.0), &img), "100x50");
    assert_eq!(shown(Some(2.0), &img), "100x50");
}

#[test]
fn x_candidates_pick_by_ratio_and_show_at_css_size() {
    let img = format!("<img id=i srcset=\"{P100} 1x, {P200} 2x, {P400} 4x\">");
    // ratio 1 -> the 1x image (100 px, shown 100); ratio 2 -> the 2x image (200 px, shown 100).
    assert_eq!(shown(Some(1.0), &img), "100x50");
    assert_eq!(shown(Some(2.0), &img), "100x50");
}

#[test]
fn w_candidates_use_sizes_for_the_pick_and_the_shown_size() {
    let set = format!("{P100} 100w, {P200} 200w, {P400} 400w, {P800} 800w");
    // sizes=300px: ratio 1 picks the 400w image (density 1.33), shown 300 wide.
    let img = format!("<img id=i srcset=\"{set}\" sizes=\"300px\">");
    assert_eq!(shown(Some(1.0), &img), "300x150");
    assert_eq!(shown(Some(2.0), &img), "300x150");
    // sizes with a media condition: 600px at 800 wide.
    let img = format!("<img id=i srcset=\"{set}\" sizes=\"(max-width: 500px) 100px, 600px\">");
    assert_eq!(shown(Some(1.0), &img), "600x300");
    // no sizes: 100vw = 800.
    let img = format!("<img id=i srcset=\"{set}\">");
    assert_eq!(shown(Some(1.0), &img), "800x400");
}

#[test]
fn picture_sources_choose_by_media_and_the_img_is_the_fallback() {
    let hit = format!("<picture><source media=\"(max-width: 900px)\" srcset=\"{P200}\"><img id=i src=\"{P400}\"></picture>");
    assert_eq!(shown(Some(1.0), &hit), "200x100");
    let miss = format!("<picture><source media=\"(max-width: 500px)\" srcset=\"{P200}\"><img id=i src=\"{P400}\"></picture>");
    assert_eq!(shown(Some(1.0), &miss), "400x200");
    let odd = format!("<picture><source type=\"image/avif\" srcset=\"{P100}\"><img id=i src=\"{P400}\"></picture>");
    assert_eq!(shown(Some(1.0), &odd), "400x200");
}

#[test]
fn src_with_a_1x_candidate_only_loads_what_it_names_at_ratio_1() {
    let img = format!("<img id=i src=\"{P100}\" srcset=\"{P200} 2x\">");
    assert_eq!(shown(Some(1.0), &img), "100x50");
    assert_eq!(shown(Some(2.0), &img), "100x50");
}

#[test]
fn an_embedder_that_gave_no_ratio_still_gets_the_densest_candidate_at_its_css_size() {
    // What the engine did before (the densest wins) now also lays out at CSS size.
    let img = format!("<img id=i srcset=\"{P100} 1x, {P200} 2x\">");
    assert_eq!(shown(None, &img), "100x50");
}
