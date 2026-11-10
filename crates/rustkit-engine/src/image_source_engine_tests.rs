//! E54b: the engine wires `image_source::select_source` into layout: an `<img>`'s box is the
//! picked image's decoded size over its density (a 200 px image picked as 2x is 100 CSS px wide,
//! as Chrome's `naturalWidth` says), the `<picture>` source is used, and a page that offers only
//! `srcset` still shows an image. The images are `data:` PNGs of 100, 200 and 400 px width.

use super::*;

const P100: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAGQAAAAyCAIAAAAlV+npAAAAX0lEQVR42u3QMQEAAAQAMJwSiyiWCgJsEZY9G/yUAlmyZMmSJUuBLFmyZMmSpUCWLFmyZMlSIEuWLFmyZCmQJUuWLFmyFMiSJUuWLFkKZMmSJUuWLAWyZMmSJUuWgr8DgGkBrsCUGxcAAAAASUVORK5CYII=";
const P200: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAMgAAABkCAIAAABM5OhcAAAAy0lEQVR42u3SMREAMAgAsdIRxUhEFiYYGBIJfx9Z/WDblwBjYSyMBcbCWBgLjIWxMBYYC2NhLDAWxsJYYCyMhbHAWBgLY4GxMBbGAmNhLIwFxsJYGAuMhbEwFhgLY2EsMBbGwlhgLIyFscBYGAtjgbEwFsYCY2EsjAXGwlgYC4yFsTAWGAtjYSwwFsbCWGAsjIWxwFgYC2OBsTAWxgJjYSyMBcbCWBgLjIWxMBYYC2NhLDAWxsJYGAuMhbEwFhgLY2EsMBbGwlhgLG4bU88CElBOZOUAAAAASUVORK5CYII=";
const P400: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAZAAAADICAIAAABJdyC1AAACBElEQVR42u3UMQ0AQAjAwOdHFCMRWahgILmT0KGR1Q/ggi8BYFgAhgUYFoBhARgWYFgAhgVgWIBhARgWgGEBhgVgWACGBRgWgGEBGBZgWACGBWBYgGEBGBaAYQGGBWBYAIYFGBaAYQGGBWBYAIYFGBaAYQEYFmBYAIYFYFiAYQEYFoBhAYYFYFgAhgUYFoBhARgWYFgAhgVgWIBhARgWgGEBhgVgWACGBRgWgGEBhgVgWACGBRgWgGEBGBZgWACGBWBYgGEBGBaAYQGGBWBYAIYFGBaAYQEYFmBYAIYFYFiAYQEYFoBhAYYFYFgAhgUYFoBhAYYFYFgAhgUYFoBhARgWYFgAhgVgWIBhARgWgGEBhgVgWACGBRgWgGEBGBZgWACGBWBYgGEBGBaAYQGGBWBYAIYFGBaAYQGGBWBYAIYFGBaAYQEYFmBYAIYFYFiAYQEYFoBhAYYFYFgAhgUYFoBhARgWYFgAhgVgWIBhARgWgGEBhgVgWACGBRgWgGEBhgVgWACGBRgWgGEBGBZgWACGBWBYgGEBGBaAYQGGBWBYAIYFGBaAYQEYFmBYAIYFYFiAYQEYFoBhAYYFYFgAhgUYFoBhAYYFYFgAhgUYFoBhARgWYFgAhgVgWIBhARgWgGEBhgVgWACGBRgWgGEBGBZgWACGBWBYgGEBGBaAYQGGBbBnAKIMAtqV+NfbAAAAAElFTkSuQmCC";

fn width_of(page: &str, dpr: f32, viewport_w: u32) -> f64 {
    let config = EngineConfig { device_pixel_ratio: dpr, ..EngineConfig::default() };
    let mut engine = Engine::new(config).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: viewport_w, height: 600 })
        .expect("view");
    engine.load_html(view, page).expect("load_html");
    let out = engine
        .execute_script(view, "String(document.getElementById('t').getBoundingClientRect().width)")
        .unwrap();
    out.strip_prefix("String(\"")
        .and_then(|v| v.strip_suffix("\")"))
        .unwrap_or(&out)
        .parse()
        .unwrap_or(f64::NAN)
}

fn page(body: String) -> String {
    format!("<html><body style='margin:0'>{body}</body></html>")
}

#[test]
fn a_2x_pick_of_a_200px_image_is_100_css_px_wide() {
    let body = format!("<img id=t srcset=\"{P100} 1x, {P200} 2x\">");
    let w = width_of(&page(body.clone()), 2.0, 800);
    assert!((w - 100.0).abs() < 0.5, "at DPR 2 the 2x image is 100 CSS px, got {w}");
    assert!((width_of(&page(body), 1.0, 800) - 100.0).abs() < 0.5, "at DPR 1 the 1x image is 100 CSS px");
}

#[test]
fn a_page_with_only_a_srcset_still_shows_an_image() {
    // The original hole: no src, one 2x candidate. It is picked at DPR 1 (the largest covers
    // nothing smaller) and its box is the decoded 200 px over density 2.
    let body = format!("<img id=t srcset=\"{P200} 2x\">");
    assert!((width_of(&page(body), 1.0, 800) - 100.0).abs() < 0.5);
}

#[test]
fn a_picture_source_is_used_when_its_media_matches() {
    let body = format!(
        "<picture><source media=\"(min-width: 600px)\" srcset=\"{P400}\"><img id=t src=\"{P100}\"></picture>"
    );
    assert!((width_of(&page(body.clone()), 1.0, 800) - 400.0).abs() < 0.5, "a wide viewport takes the source");
    assert!((width_of(&page(body), 1.0, 400) - 100.0).abs() < 0.5, "a narrow one takes the img's src");
}
