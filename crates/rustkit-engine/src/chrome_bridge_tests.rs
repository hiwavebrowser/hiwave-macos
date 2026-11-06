//! The app's state bridge into the RustKit chrome (CH3.A10).
//!
//! While a page loads the app tells the chrome the URL, the title, the
//! back/forward state, the loading flag, the shield counters and the whole
//! tab strip and sidebar. Until CH3.A10 each was its own script
//! (`updateUrl`, `renderTabs`, `renderWorkspaces`, ...), one script task and
//! one full relayout of chrome.html each, ~14 per navigation, and
//! `renderTabs`/`renderWorkspaces` rebuilt the strip and the sidebar with
//! `innerHTML` every time (batch 28: ~1,330 `relayout cause=script_mutation`
//! on the chrome view per 20 s load loop).
//!
//! The app now sends what changed in one turn as one script,
//! `__hiwaveApplyState({...})`; the page keeps the latest value of each key,
//! applies them once per animation frame, and writes only what differs
//! from the document (a tab's title text, a class), so a run of updates is
//! at most one relayout per frame and an update that changes nothing is
//! none.

use super::*;

const CHROME: &str = include_str!("../../hiwave-app/src/ui/chrome.html");

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn chrome() -> (Engine, EngineViewId) {
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds {
            x: 0,
            y: 0,
            width: 1280,
            height: 120,
        })
        .expect("view");
    // The app's own document, as the RustKit chrome is: it gets the app
    // bridge (`window.ipc`), which chrome.html's boot posts on.
    engine.set_app_owned(view, true);
    engine.load_html(view, CHROME).expect("load_html");
    // The chrome's own boot (entrance animations, its first IPC) settles.
    for _ in 0..40 {
        frame(&mut engine, view);
    }
    (engine, view)
}

/// One live turn of 16 ms: an animation frame.
fn frame(engine: &mut Engine, view: EngineViewId) {
    rt().block_on(engine.pump_live(view, 16));
}

fn run(engine: &mut Engine, view: EngineViewId, script: &str) {
    engine.execute_script(view, script).unwrap();
}

fn read(engine: &mut Engine, view: EngineViewId, expr: &str) -> String {
    let value = engine
        .execute_script(view, &format!("String({expr})"))
        .unwrap();
    value
        .strip_prefix("String(\"")
        .and_then(|v| v.strip_suffix("\")"))
        .unwrap_or(&value)
        .to_string()
}

fn relayouts(engine: &Engine, view: EngineViewId) -> u32 {
    engine.relayout_stats(view).expect("view").relayouts
}

/// The script the app sends for one turn's state (hiwave-app
/// `chrome_state::bridge_script`): the chrome's applier, or, before the
/// chrome's script has run, a queue it drains when it does.
fn bridge(state: &str) -> String {
    format!(
        "(window.__hiwaveApplyState||function(s){{(window.__hiwaveEarlyState=window.__hiwaveEarlyState||[]).push(s)}})({state})"
    )
}

fn tabs(active_title: &str, active: u32) -> String {
    let tab = |id: u32, title: &str| {
        format!(
            r#"{{"id":{id},"title":"{title}","url":"http://127.0.0.1/p{id}","is_active":{},"locked":false,"last_visited":{},"decay_level":0}}"#,
            id == active,
            1_700_000_000 + id
        )
    };
    format!("[{},{},{}]", tab(1, if active == 1 { active_title } else { "One" }), tab(2, if active == 2 { active_title } else { "Two" }), tab(3, if active == 3 { active_title } else { "Three" }))
}

fn workspaces(active_title: &str, active: u32) -> String {
    let tab = |id: u32, title: &str| {
        format!(
            r#"{{"id":"{id}","title":"{title}","url":"http://127.0.0.1/p{id}","is_active":{},"locked":false,"decay_level":0}}"#,
            id == active
        )
    };
    format!(
        r#"[{{"id":"w1","name":"Default","tab_count":3,"tabs":[{},{},{}],"is_active":true,"is_suspended":false,"locked_count":0}}]"#,
        tab(1, if active == 1 { active_title } else { "One" }),
        tab(2, if active == 2 { active_title } else { "Two" }),
        tab(3, if active == 3 { active_title } else { "Three" })
    )
}

fn full_state(title: &str, loading: bool, blocked: u32) -> String {
    format!(
        r#"{{"tabs":{},"workspaces":{},"url":"http://127.0.0.1/p2","title":"{title}","nav":[true,false],"loading":{loading},"shield":{{"enabled":true,"requests_blocked":{blocked},"trackers_blocked":0}}}}"#,
        tabs(title, 2),
        workspaces(title, 2)
    )
}

/// 100 progress updates in one frame (the loading flag, the shield
/// counters climbing, the same tabs and title) lay the chrome out at most
/// twice, and the last of each value is what the chrome shows.
#[test]
fn a_hundred_progress_updates_lay_the_chrome_out_at_most_twice() {
    let (mut engine, view) = chrome();
    run(&mut engine, view, &bridge(&full_state("Page", false, 0)));
    for _ in 0..3 {
        frame(&mut engine, view);
    }
    let before = relayouts(&engine, view);
    for i in 1..=100 {
        run(&mut engine, view, &bridge(&format!(
            r#"{{"loading":true,"title":"Page","shield":{{"enabled":true,"requests_blocked":{i},"trackers_blocked":0}}}}"#
        )));
    }
    for _ in 0..3 {
        frame(&mut engine, view);
    }
    let laid = relayouts(&engine, view) - before;
    assert!(laid <= 2, "100 progress updates laid the chrome out {laid} times");
    assert_eq!(read(&mut engine, view, "document.getElementById('shieldCount').textContent"), "100");
    assert_eq!(
        read(&mut engine, view, "document.getElementById('loadingIndicator').classList.contains('active')"),
        "true"
    );
}

/// The same state sent every frame (what a load's repeated syncs amount
/// to) changes nothing in the document after the first frame.
#[test]
fn unchanged_state_every_frame_does_not_lay_the_chrome_out() {
    let (mut engine, view) = chrome();
    run(&mut engine, view, &bridge(&full_state("Page", true, 5)));
    for _ in 0..3 {
        frame(&mut engine, view);
    }
    assert_eq!(read(&mut engine, view, "document.querySelector('.tab.active .tab-title').textContent"), "Page");
    let before = relayouts(&engine, view);
    for _ in 0..100 {
        run(&mut engine, view, &bridge(&full_state("Page", true, 5)));
        frame(&mut engine, view);
    }
    let laid = relayouts(&engine, view) - before;
    assert!(laid <= 2, "100 unchanged updates laid the chrome out {laid} times");
}

/// A new title updates the active tab's text in place: the strip is not
/// rebuilt (the tab element is the same node), the other tabs keep theirs.
#[test]
fn a_new_title_updates_the_tab_in_place() {
    let (mut engine, view) = chrome();
    run(&mut engine, view, &bridge(&full_state("Loading", true, 0)));
    for _ in 0..3 {
        frame(&mut engine, view);
    }
    run(&mut engine, view, "document.querySelector('.tab.active').__a10 = 7; document.querySelector('#sidebarWorkspaceList .workspace-entry') && (document.querySelector('#sidebarWorkspaceList .workspace-entry').__a10 = 7)");
    run(&mut engine, view, &bridge(&format!(
        r#"{{"title":"Done","tabs":{},"workspaces":{},"loading":false}}"#,
        tabs("Done", 2),
        workspaces("Done", 2)
    )));
    for _ in 0..3 {
        frame(&mut engine, view);
    }
    assert_eq!(read(&mut engine, view, "document.querySelector('.tab.active .tab-title').textContent"), "Done");
    assert_eq!(read(&mut engine, view, "document.querySelector('.tab.active').__a10"), "7", "the tab was rebuilt");
    assert_eq!(
        read(&mut engine, view, "Array.from(document.querySelectorAll('.tab .tab-title')).map(function(t){return t.textContent}).join('|')"),
        "One|Done|Three"
    );
    assert_eq!(read(&mut engine, view, "document.title"), "Done - HiWave");
    assert_eq!(
        read(&mut engine, view, "document.getElementById('loadingIndicator').classList.contains('active')"),
        "false"
    );
}

/// A change of tabs (one opened) still rebuilds the strip, in order, with
/// the new active tab.
#[test]
fn a_new_tab_rebuilds_the_strip() {
    let (mut engine, view) = chrome();
    run(&mut engine, view, &bridge(&full_state("Page", false, 0)));
    for _ in 0..3 {
        frame(&mut engine, view);
    }
    let four = r#"[{"id":1,"title":"One","url":"http://127.0.0.1/p1","is_active":false,"locked":false,"last_visited":1,"decay_level":0},{"id":2,"title":"Page","url":"http://127.0.0.1/p2","is_active":false,"locked":false,"last_visited":2,"decay_level":0},{"id":3,"title":"Three","url":"http://127.0.0.1/p3","is_active":false,"locked":false,"last_visited":3,"decay_level":0},{"id":4,"title":"New Tab","url":"hiwave://newtab","is_active":true,"locked":false,"last_visited":4,"decay_level":0}]"#;
    run(&mut engine, view, &bridge(&format!(r#"{{"tabs":{four}}}"#)));
    for _ in 0..3 {
        frame(&mut engine, view);
    }
    assert_eq!(read(&mut engine, view, "document.querySelectorAll('.tab').length"), "4");
    assert_eq!(read(&mut engine, view, "document.querySelector('.tab.active').dataset.id"), "4");
}

/// State sent before the chrome's script defined the applier is kept and
/// applied when it does (the app's first sync can beat the chrome's boot).
#[test]
fn state_sent_before_the_chrome_booted_is_applied() {
    let (mut engine, view) = chrome();
    // As if the applier were not there yet: the bridge queues the state;
    // the chrome's boot drains the queue (`__hiwaveDrainEarlyState`).
    run(&mut engine, view, &format!(
        "(function(){{var a=window.__hiwaveApplyState; window.__hiwaveApplyState=undefined; {}; window.__hiwaveApplyState=a; window.__hiwaveDrainEarlyState();}})()",
        bridge(r#"{"url":"http://127.0.0.1/early"}"#)
    ));
    for _ in 0..3 {
        frame(&mut engine, view);
    }
    assert_eq!(read(&mut engine, view, "document.getElementById('urlInput').value"), "http://127.0.0.1/early");
}

/// The legacy per-call scripts the app sent for one navigation before
/// CH3.A10, in the order a RustKit page's load produced them.
fn legacy_navigation(i: u32) -> Vec<String> {
    let title = format!("Page {i}");
    let shield = format!(r#"{{"enabled":true,"requests_blocked":{i},"trackers_blocked":0}}"#);
    let wrap = |call: String| format!("if(window.hiwaveChrome) {{ hiwaveChrome.{call}; }}");
    vec![
        // The load starts and commits its URL.
        wrap("setLoading(true)".into()),
        wrap("updateUrl('http://127.0.0.1/p2')".into()),
        wrap(format!("updateShieldStats({shield})")),
        wrap(format!("renderTabs({})", tabs("Page", 2))),
        wrap(format!("renderWorkspaces({})", workspaces("Page", 2))),
        wrap("updateNavState(true, false)".into()),
        // The `get_shelf` the sidebar asked for.
        wrap(r#"renderShelf({"scope":"workspace","workspace":"Default","items":[],"global_count":0})"#.into()),
        // The title.
        wrap(format!("updateTitle('{title}')")),
        wrap(format!("renderTabs({})", tabs(&title, 2))),
        wrap(format!("renderWorkspaces({})", workspaces(&title, 2))),
        wrap(r#"renderShelf({"scope":"workspace","workspace":"Default","items":[],"global_count":0})"#.into()),
        // The load ends.
        wrap("setLoading(false)".into()),
        wrap(format!("updateShieldStats({shield})")),
        wrap("updateHistory([])".into()),
    ]
}

/// The bridge scripts the app sends for the same navigation (one per turn).
fn bridged_navigation(i: u32) -> Vec<String> {
    let title = format!("Page {i}");
    let shield = format!(r#"{{"enabled":true,"requests_blocked":{i},"trackers_blocked":0}}"#);
    vec![
        bridge(&format!(
            r#"{{"loading":true,"url":"http://127.0.0.1/p2","shield":{shield},"tabs":{},"workspaces":{},"nav":[true,false]}}"#,
            tabs("Page", 2),
            workspaces("Page", 2)
        )),
        bridge(r#"{"shelf":{"scope":"workspace","workspace":"Default","items":[],"global_count":0}}"#),
        bridge(&format!(r#"{{"title":"{title}","tabs":{},"workspaces":{}}}"#, tabs(&title, 2), workspaces(&title, 2))),
        bridge(&format!(r#"{{"loading":false,"shield":{shield},"history":[]}}"#)),
    ]
}

/// Measurement (not a gate): chrome relayouts and script time for 20
/// navigations, each event in its own frame, through the legacy calls
/// and through the bridge. `cargo test -p rustkit-engine --features
/// headless chrome_bridge_measure -- --ignored --nocapture`.
#[test]
#[ignore]
fn chrome_bridge_measure() {
    for (name, legacy) in [("legacy calls", true), ("bridge", false)] {
        let (mut engine, view) = chrome();
        let (before, mut scripts, started) = (relayouts(&engine, view), 0, std::time::Instant::now());
        for i in 0..20 {
            let batch = if legacy { legacy_navigation(i) } else { bridged_navigation(i) };
            for script in batch {
                scripts += 1;
                if engine.execute_script(view, &script).is_err() {
                    eprintln!("{name}: script failed (no applier on this chrome.html)");
                }
                frame(&mut engine, view);
            }
        }
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        let stats = engine.relayout_stats(view).expect("view");
        eprintln!(
            "A10 {name}: 20 navigations, {scripts} scripts, {} relayouts ({} script_mutation), total {:.1} ms wall, relayout total_ms {:.1}",
            relayouts(&engine, view) - before,
            stats.by_first_cause.get("script_mutation"),
            ms,
            stats.total_ms
        );
    }
}
