//! CH2.E6: a RustKit-drawn `<select>` drop-down, through the engine's
//! user-input entry points (press, release, move, keys) on a headless view.
//!
//! Each test needs a GPU device to make an engine; where there is none
//! (a Linux container) it prints why and passes, as `consent_click_tests`
//! does. The popup's model is covered everywhere by `select_popup_tests`,
//! and the pick through script by rustkit-bindings' `form_controls_tests`.

use super::*;

const PAGE: &str = r#"<!DOCTYPE html><html><body style="margin:0;height:600px">
<select id="s" style="position:absolute;left:10px;top:10px;width:150px;height:24px;font-size:13px">
  <option>Zero</option>
  <option disabled>One (off)</option>
  <optgroup label="Group"><option>Two</option><option>Three</option></optgroup>
  <option>Four</option>
</select>
<select id="low" style="position:absolute;left:10px;top:570px;width:150px;height:24px">
  <option>a</option><option>b</option><option>c</option>
</select>
<select id="box" size="3" style="position:absolute;left:300px;top:10px;width:100px">
  <option>x</option><option>y</option><option>z</option>
</select>
<div id="other" style="position:absolute;left:400px;top:300px;width:100px;height:100px"></div>
<script>
  window.log = [];
  var s = document.getElementById('s');
  ['input', 'change'].forEach(function (t) {
    document.addEventListener(t, function (e) { log.push(t + ':' + e.target.id + ':' + e.target.selectedIndex); });
  });
  document.addEventListener('keydown', function (e) { log.push('keydown:' + e.key); });
</script>
</body></html>"#;

fn page() -> Option<(Engine, EngineViewId)> {
    let mut engine = match Engine::new(EngineConfig::default()) {
        Ok(engine) => engine,
        Err(e) => {
            eprintln!("Skipping test: no engine ({e:?})");
            return None;
        }
    };
    let id = engine.create_headless_view(Bounds::new(0, 0, 800, 600)).expect("headless view");
    engine.load_html(id, PAGE).expect("load_html");
    Some((engine, id))
}

fn js(engine: &mut Engine, id: EngineViewId, script: &str) -> String {
    let out = engine.execute_script(id, script).expect("script");
    out.strip_prefix("String(\"")
        .and_then(|s| s.strip_suffix("\")"))
        .map_or(out.clone(), str::to_string)
}

fn click(engine: &mut Engine, id: EngineViewId, x: f32, y: f32) {
    engine.mouse_down_at_point(id, x, y);
    engine.click_at_point(id, x, y);
}

/// Click the middle of the select `s`.
fn open(engine: &mut Engine, id: EngineViewId) {
    click(engine, id, 85.0, 22.0);
    assert!(engine.select_popup_open(id), "a click on the select opens its popup");
}

fn texts(engine: &Engine, id: EngineViewId) -> Vec<String> {
    let list = engine.views[&id].display_list.clone().expect("display list");
    list.commands
        .iter()
        .filter_map(|c| match c {
            rustkit_layout::DisplayCommand::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

/// The middle of option `index`'s row in the open popup, in viewport
/// coordinates (the page is not scrolled).
fn row_point(engine: &Engine, id: EngineViewId, index: usize) -> (f32, f32) {
    let popup = engine.views[&id].lay.select_popup.borrow().clone().expect("open popup");
    let row = popup
        .rows
        .iter()
        .position(|(r, _)| *r == select_popup::Row::Option(index))
        .expect("a row");
    let y = popup.rect.y + 1.0 + popup.row_height * ((row - popup.first_row) as f32 + 0.5);
    (popup.rect.x + 20.0, y)
}

fn log(engine: &mut Engine, id: EngineViewId) -> String {
    js(engine, id, "log.join(' ')")
}

#[test]
fn a_click_opens_the_popup_and_the_display_list_holds_its_rows() {
    let Some((mut engine, id)) = page() else { return };
    let before = texts(&engine, id);
    open(&mut engine, id);
    let after = texts(&engine, id);
    let added: Vec<&String> = after.iter().skip(before.len()).collect();
    assert_eq!(added, ["Zero", "One (off)", "Group", "Two", "Three", "Four"], "the rows follow the page");
    let popup = engine.views[&id].lay.select_popup.borrow().clone().unwrap();
    assert_eq!(popup.rect.y, 34.0, "the popup hangs under the control");
    assert_eq!(popup.highlighted, Some(0), "the selected option is highlighted");
}

#[test]
fn a_click_on_the_third_option_selects_it_and_fires_change_once() {
    let Some((mut engine, id)) = page() else { return };
    open(&mut engine, id);
    let (x, y) = row_point(&engine, id, 2);
    click(&mut engine, id, x, y);
    assert!(!engine.select_popup_open(id), "a pick closes the popup");
    assert_eq!(js(&mut engine, id, "String(s.selectedIndex) + s.value"), "2Two");
    assert_eq!(log(&mut engine, id), "input:s:2 change:s:2");
    assert_eq!(js(&mut engine, id, "String(s.options[2].matches(':checked'))"), "true");
    // The closed control shows it.
    assert!(texts(&engine, id).iter().all(|t| t != "Zero"), "the label moved off Zero");
}

#[test]
fn escape_closes_without_a_change() {
    let Some((mut engine, id)) = page() else { return };
    open(&mut engine, id);
    assert!(engine.handle_text_key(id, 0x28, "", false, false, false), "ArrowDown moves the highlight");
    assert!(engine.handle_text_key(id, 0x1B, "", false, false, false));
    assert!(!engine.select_popup_open(id));
    assert_eq!(js(&mut engine, id, "String(s.selectedIndex)"), "0");
    assert_eq!(log(&mut engine, id), "", "no change, and the page heard no key while the popup was open");
    let before = texts(&engine, id);
    assert!(!before.contains(&"Group".to_string()), "the rows are gone from the display list");
}

#[test]
fn arrow_down_and_enter_select_the_next_enabled_option() {
    let Some((mut engine, id)) = page() else { return };
    open(&mut engine, id);
    engine.handle_text_key(id, 0x28, "", false, false, false);
    assert_eq!(engine.submit_focused_form(id), None, "Enter in the popup submits nothing");
    engine.handle_text_key(id, 0x0D, "", false, false, false);
    assert!(!engine.select_popup_open(id));
    // Option 1 is disabled: the next is 2.
    assert_eq!(js(&mut engine, id, "String(s.selectedIndex)"), "2");
    assert_eq!(log(&mut engine, id), "input:s:2 change:s:2");
}

#[test]
fn a_disabled_option_cannot_be_selected() {
    let Some((mut engine, id)) = page() else { return };
    open(&mut engine, id);
    let (x, y) = row_point(&engine, id, 1);
    engine.mouse_move_at_point(id, x, y);
    click(&mut engine, id, x, y);
    assert!(engine.select_popup_open(id), "a click on a disabled option leaves the popup open");
    assert_eq!(js(&mut engine, id, "String(s.selectedIndex)"), "0");
    assert_eq!(log(&mut engine, id), "");
}

#[test]
fn a_click_elsewhere_closes_and_reaches_nothing() {
    let Some((mut engine, id)) = page() else { return };
    js(&mut engine, id, "document.getElementById('other').onclick = function () { log.push('other'); }; ''");
    open(&mut engine, id);
    click(&mut engine, id, 450.0, 350.0);
    assert!(!engine.select_popup_open(id));
    assert_eq!(log(&mut engine, id), "", "no change, and the click that closed the popup went nowhere");
    // A click on the select while open closes it without reopening.
    open(&mut engine, id);
    click(&mut engine, id, 85.0, 22.0);
    assert!(!engine.select_popup_open(id));
}

#[test]
fn hover_moves_the_highlight() {
    let Some((mut engine, id)) = page() else { return };
    open(&mut engine, id);
    let (x, y) = row_point(&engine, id, 3);
    engine.mouse_move_at_point(id, x, y);
    let popup = engine.views[&id].lay.select_popup.borrow().clone().unwrap();
    assert_eq!(popup.highlighted, Some(3));
}

#[test]
fn closed_arrow_keys_and_letters_change_the_value_directly() {
    let Some((mut engine, id)) = page() else { return };
    js(&mut engine, id, "s.focus(); ''");
    assert!(engine.handle_text_key(id, 0x28, "", false, false, false));
    assert!(!engine.select_popup_open(id), "an arrow key does not open a closed select");
    assert_eq!(js(&mut engine, id, "String(s.selectedIndex)"), "2");
    assert!(engine.handle_text_key(id, 0, "f", false, false, false));
    assert_eq!(js(&mut engine, id, "String(s.selectedIndex)"), "4");
    assert_eq!(log(&mut engine, id), "keydown:ArrowDown input:s:2 change:s:2 keydown:f input:s:4 change:s:4");
    // Space opens it.
    assert!(engine.handle_text_key(id, 0, " ", false, false, false));
    assert!(engine.select_popup_open(id));
}

#[test]
fn the_popup_opens_above_a_control_near_the_bottom() {
    let Some((mut engine, id)) = page() else { return };
    click(&mut engine, id, 85.0, 582.0);
    let popup = engine.views[&id].lay.select_popup.borrow().clone().expect("open");
    assert_eq!(popup.rect.y + popup.rect.height, 570.0);
}

#[test]
fn a_list_box_does_not_pop_up() {
    let Some((mut engine, id)) = page() else { return };
    click(&mut engine, id, 350.0, 20.0);
    assert!(!engine.select_popup_open(id));
}

