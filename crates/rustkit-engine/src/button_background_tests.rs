//! CH2.B3: an author's `background: transparent` on a button survives to
//! paint.
//!
//! chrome.html's workspace expanders and tab lock buttons are
//! `border: none; background: transparent`, and Chromium 143 computes
//! `background-color: rgba(0, 0, 0, 0)` for all of them (chrome board,
//! default state). The painter used to swap any alpha-0 button background
//! for ButtonFace (239, 239, 239), so every one painted as a white block.
//! ButtonFace is the UA default, so it belongs in the UA style, where an
//! author rule can override it (the #83 fix for fields, extended to buttons).
//!
//! Page views (no GPU texture): the display list is read as built.

use super::*;
use rustkit_css::Color;
use rustkit_layout::DisplayCommand;

const BUTTON_FACE: Color = Color { r: 239, g: 239, b: 239, a: 1.0 };

fn button_backgrounds(html: &str) -> Vec<(String, Color)> {
    let mut engine =
        Engine::new_page(EngineConfig::default(), None, Arc::new(AtomicU64::new(0))).expect("engine");
    let view = engine.create_page_view(Bounds::new(0, 0, 600, 400)).expect("page view");
    engine.load_html(view, html).expect("load");
    engine.views[&view]
        .display_list
        .as_ref()
        .expect("display list")
        .commands
        .iter()
        .filter_map(|c| match c {
            DisplayCommand::Button { label, background_color, .. } => Some((label.clone(), *background_color)),
            _ => None,
        })
        .collect()
}

fn background_of(list: &[(String, Color)], label: &str) -> Color {
    list.iter()
        .find(|(l, _)| l.trim() == label)
        .unwrap_or_else(|| panic!("no button labelled {label:?} in {list:?}"))
        .1
}

#[test]
fn a_transparent_button_paints_no_background() {
    let list = button_backgrounds(
        r#"<body style="background:#1e293b">
        <button style="border:none;background:transparent;padding:4px">v</button>
        <button style="border:none;background:transparent;display:flex;width:28px;height:28px">L</button>
        <input type="button" value="I" style="border:none;background-color:transparent">
        </body>"#,
    );
    for label in ["v", "L", "I"] {
        let bg = background_of(&list, label);
        assert_eq!(bg.a, 0.0, "button {label:?}: Chromium paints none, got {bg:?}");
    }
}

#[test]
fn an_unstyled_button_keeps_buttonface() {
    let list = button_backgrounds(
        r#"<body><button>plain</button><input type="submit" value="Go"><input type="reset" value="R"></body>"#,
    );
    for label in ["plain", "Go", "R"] {
        assert_eq!(background_of(&list, label), BUTTON_FACE, "button {label:?}");
    }
}

#[test]
fn an_authored_button_colour_is_kept() {
    let list = button_backgrounds(r#"<body><button style="background:rgb(1,2,3)">c</button></body>"#);
    assert_eq!(background_of(&list, "c"), Color { r: 1, g: 2, b: 3, a: 1.0 });
}
