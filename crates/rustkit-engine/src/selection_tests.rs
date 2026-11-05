//! CH2.E2: the selection model, mouse selection and the painted highlight.
//!
//! Page views (no GPU texture): the display list is read as built.

use super::*;
use rustkit_layout::{DisplayCommand, SELECTION_COLOR};

fn page(html: &str) -> (Engine, EngineViewId) {
    let mut engine = Engine::new_page(EngineConfig::default(), None, Arc::new(AtomicU64::new(0))).expect("engine");
    let view = engine.create_page_view(Bounds::new(0, 0, 600, 400)).expect("page view");
    engine.load_html(view, html).expect("load");
    (engine, view)
}

fn read(engine: &mut Engine, view: EngineViewId, expr: &str) -> String {
    let value = engine.execute_script(view, &format!("String({expr})")).unwrap();
    value
        .strip_prefix("String(\"")
        .and_then(|v| v.strip_suffix("\")"))
        .unwrap_or(&value)
        .to_string()
}

fn commands(engine: &Engine, view: EngineViewId) -> Vec<DisplayCommand> {
    engine.views[&view].display_list.as_ref().expect("display list").commands.clone()
}

/// The selection rects painted, as `(x, y, width, height)`.
fn highlight(engine: &Engine, view: EngineViewId) -> Vec<(f32, f32, f32, f32)> {
    commands(engine, view)
        .iter()
        .filter_map(|c| match c {
            DisplayCommand::SolidColor(color, r) if *color == SELECTION_COLOR => Some((r.x, r.y, r.width, r.height)),
            _ => None,
        })
        .collect()
}

/// Where character boundary `index` of the painted run containing `needle`
/// is, and the line's middle y.
fn char_x(engine: &Engine, view: EngineViewId, needle: &str, index: usize) -> (f32, f32) {
    for c in commands(engine, view) {
        if let DisplayCommand::Text { text, x, y, advances, font_size, font_family, .. } = c {
            if text.contains(needle) {
                let adv = advances.unwrap_or_else(|| {
                    text.chars()
                        .map(|ch| rustkit_layout::measure_text_advanced(&ch.to_string(), &font_family, font_size, Default::default(), Default::default()).width)
                        .collect()
                });
                return (x + adv[..index].iter().sum::<f32>(), y + font_size / 2.0);
            }
        }
    }
    panic!("no text run with {needle:?}");
}

const PARAGRAPH: &str = "<html><body style='margin:0'><p id=p style='margin:0;font-size:20px'>alpha beta gamma</p>\
    <p id=q style='margin:0;font-size:20px'>second <b>line</b> here</p></body></html>";

#[test]
fn dragging_across_two_words_selects_and_paints_them() {
    let (mut engine, view) = page(PARAGRAPH);
    assert!(highlight(&engine, view).is_empty(), "nothing selected yet");
    let (x0, y) = char_x(&engine, view, "alpha", 0);
    let (x10, _) = char_x(&engine, view, "alpha", 10);
    engine.mouse_move_at_point(view, x0 + 1.0, y);
    engine.mouse_down_at_point(view, x0 + 1.0, y);
    engine.mouse_move_at_point(view, (x0 + x10) / 2.0, y);
    engine.mouse_move_at_point(view, x10 + 1.0, y);
    engine.click_at_point(view, x10 + 1.0, y);
    assert_eq!(read(&mut engine, view, "getSelection().toString()"), "alpha beta");
    assert_eq!(read(&mut engine, view, "getSelection().anchorOffset + ',' + getSelection().focusOffset"), "0,10");
    let rects = highlight(&engine, view);
    assert_eq!(rects.len(), 1, "{rects:?}");
    let (rx, _, rw, rh) = rects[0];
    assert!((rx - x0).abs() < 0.5 && (rx + rw - x10).abs() < 0.5, "{rects:?} vs {x0}..{x10}");
    assert!(rh > 0.0);
    // The highlight is behind the glyphs.
    let list = commands(&engine, view);
    let fill = list.iter().position(|c| matches!(c, DisplayCommand::SolidColor(color, _) if *color == SELECTION_COLOR));
    let text = list.iter().position(|c| matches!(c, DisplayCommand::Text { text, .. } if text.contains("alpha")));
    assert!(fill < text, "selection before text");
}

#[test]
fn a_drag_across_paragraphs_paints_each_run() {
    let (mut engine, view) = page(PARAGRAPH);
    let (x6, y) = char_x(&engine, view, "alpha", 6);
    let (x2, y2) = char_x(&engine, view, "second", 2);
    engine.mouse_down_at_point(view, x6 + 0.5, y);
    engine.mouse_move_at_point(view, x2 + 0.5, y2);
    engine.click_at_point(view, x2 + 0.5, y2);
    assert_eq!(read(&mut engine, view, "getSelection().toString()"), "beta gammase");
    assert_eq!(highlight(&engine, view).len(), 2);
}

#[test]
fn double_click_selects_a_word_and_triple_click_the_paragraph() {
    let (mut engine, view) = page(PARAGRAPH);
    read(&mut engine, view, "window.__dbl = 0, document.addEventListener('dblclick', function () { window.__dbl++; })");
    let (x8, y) = char_x(&engine, view, "alpha", 8);
    engine.mouse_down_with(view, x8, y, 1, false);
    engine.click_at_point(view, x8, y);
    engine.mouse_down_with(view, x8, y, 2, false);
    engine.click_at_point(view, x8, y);
    assert_eq!(read(&mut engine, view, "getSelection().toString()"), "beta");
    assert_eq!(read(&mut engine, view, "window.__dbl"), "1");
    assert_eq!(highlight(&engine, view).len(), 1);
    engine.mouse_down_with(view, x8, y, 3, false);
    engine.click_at_point(view, x8, y);
    assert_eq!(read(&mut engine, view, "getSelection().toString()"), "alpha beta gamma");
    assert_eq!(read(&mut engine, view, "getSelection().anchorNode.id"), "p");
}

#[test]
fn shift_click_extends_the_selection() {
    let (mut engine, view) = page(PARAGRAPH);
    let (x6, y) = char_x(&engine, view, "alpha", 6);
    let (x16, _) = char_x(&engine, view, "alpha", 16);
    engine.mouse_down_at_point(view, x6 + 0.5, y);
    engine.click_at_point(view, x6 + 0.5, y);
    assert_eq!(read(&mut engine, view, "getSelection().isCollapsed"), "true");
    engine.mouse_down_with(view, x16 + 1.0, y, 1, true);
    engine.click_at_point(view, x16 + 1.0, y);
    assert_eq!(read(&mut engine, view, "getSelection().toString()"), "beta gamma");
}

#[test]
fn add_range_paints_and_fires_selectionchange() {
    let (mut engine, view) = page(PARAGRAPH);
    read(&mut engine, view, "window.__changes = 0, document.addEventListener('selectionchange', function () { window.__changes++; })");
    read(
        &mut engine,
        view,
        "(function () { var r = document.createRange(); r.selectNodeContents(document.querySelector('b')); getSelection().addRange(r); })()",
    );
    assert_eq!(read(&mut engine, view, "window.__changes"), "1");
    assert_eq!(read(&mut engine, view, "getSelection().toString()"), "line");
    let rects = highlight(&engine, view);
    assert_eq!(rects.len(), 1, "{rects:?}");
    let (bx, _) = char_x(&engine, view, "line", 0);
    assert!((rects[0].0 - bx).abs() < 0.5, "{rects:?} at {bx}");
    read(&mut engine, view, "getSelection().removeAllRanges()");
    assert!(highlight(&engine, view).is_empty());
    assert_eq!(read(&mut engine, view, "window.__changes"), "2");
}

fn text_input(engine: &Engine, view: EngineViewId) -> (String, Option<(usize, usize)>) {
    commands(engine, view)
        .into_iter()
        .find_map(|c| match c {
            DisplayCommand::TextInput { value, selection, kind: rustkit_layout::TextControlKind::Text, .. } => Some((value, selection)),
            _ => None,
        })
        .expect("a text input")
}

const FIELD: &str = "<html><body style='margin:0'><input id=f value='hello world' style='font-size:16px;width:300px'></body></html>";

#[test]
fn select_all_in_an_input_paints_one_rect_spanning_the_text() {
    let (mut engine, view) = page(FIELD);
    engine.mouse_down_at_point(view, 20.0, 10.0);
    engine.click_at_point(view, 20.0, 10.0);
    assert_eq!(text_input(&engine, view).1, None, "a caret, nothing selected");
    engine.handle_text_key(view, 0x41, "a", true, false, false);
    engine.relayout(view).unwrap();
    let (value, selection) = text_input(&engine, view);
    assert_eq!(selection, Some((0, 11)));
    let measure = |t: &str| t.chars().count() as f32 * 8.0;
    let rects = rustkit_layout::forms::calculate_selection_rects(&value, 0, 11, 2.0, 2.0, 18.0, &measure);
    assert_eq!(rects.len(), 1);
    assert_eq!((rects[0].x, rects[0].width), (2.0, 88.0), "the whole value");
}

#[test]
fn dragging_in_an_input_selects_part_of_its_value() {
    let (mut engine, view) = page(FIELD);
    let content_x = {
        let tree = engine.views[&view].layout.clone().unwrap();
        let raw = engine.views[&view].document.as_ref().unwrap().get_element_by_id("f").unwrap().id.raw();
        box_of_node(&tree, raw).unwrap().dimensions.content.x
    };
    engine.mouse_down_at_point(view, content_x + 0.5, 10.0);
    engine.mouse_move_at_point(view, content_x + 150.0, 10.0);
    let (_, selection) = text_input(&engine, view);
    let (s, e) = selection.expect("a selection");
    assert_eq!(s, 0);
    assert!(e > 0 && e <= 11, "{e}");
    engine.click_at_point(view, content_x + 150.0, 10.0);
    // A double-click selects the word under the pointer.
    engine.mouse_down_with(view, content_x + 1.0, 10.0, 2, false);
    engine.click_at_point(view, content_x + 1.0, 10.0);
    assert_eq!(text_input(&engine, view).1, Some((0, 5)));
}
