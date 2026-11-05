//! CH2.B5: a `<select>`'s value set from script shows in paint.
//!
//! settings.html sets `theme.value`, `defaultMode.value`, `tabDecayUnit.value`
//! and the analytics selects from its loaded settings; the board's first run
//! (develop 209f6a64) painted the markup's default option instead. Chromium
//! 143 shows the option the script chose, for `value`, `selectedIndex` and
//! `option.selected`, set during load or after it.

use super::*;
use rustkit_layout::DisplayCommand;

const PAGE: &str = "<body><select id=s><option value=a>Alpha</option>\
    <option value=b>Beta</option><option value=c>Gamma</option></select>\
    <script>SCRIPT</script></body>";

fn shown(engine: &Engine, view: EngineViewId) -> Vec<String> {
    engine.views[&view]
        .display_list
        .as_ref()
        .expect("display list")
        .commands
        .iter()
        .filter_map(|c| match c {
            DisplayCommand::Text { text, .. } => Some(text.clone()),
            DisplayCommand::TextInput { value, .. } => Some(value.clone()),
            _ => None,
        })
        .filter(|t| ["Alpha", "Beta", "Gamma"].contains(&t.trim()))
        .collect()
}

fn page(script: &str) -> (Engine, EngineViewId) {
    let mut engine =
        Engine::new_page(EngineConfig::default(), None, Arc::new(AtomicU64::new(0))).expect("engine");
    let view = engine.create_page_view(Bounds::new(0, 0, 600, 400)).expect("page view");
    engine.load_html(view, &PAGE.replace("SCRIPT", script)).expect("load");
    (engine, view)
}

#[test]
fn value_set_during_load_is_painted() {
    let (engine, view) = page("document.getElementById('s').value = 'b';");
    assert_eq!(shown(&engine, view), ["Beta"]);
}

#[test]
fn selected_index_set_during_load_is_painted() {
    let (engine, view) = page("document.getElementById('s').selectedIndex = 2;");
    assert_eq!(shown(&engine, view), ["Gamma"]);
}

#[test]
fn value_set_after_load_is_painted() {
    let (mut engine, view) = page("");
    assert_eq!(shown(&engine, view), ["Alpha"]);
    engine.execute_script(view, "document.getElementById('s').value = 'c'").unwrap();
    assert_eq!(shown(&engine, view), ["Gamma"]);
}
