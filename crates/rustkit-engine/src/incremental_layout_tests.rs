//! INC5 stage 1 (`docs/design/INCREMENTAL_LAYOUT_PLAN.md`): incremental
//! layout of one block box whose content changed, behind
//! `RUSTKIT_INCREMENTAL_LAYOUT`. The oracle is the full layout of the same
//! DOM (`rustkit_layout::incremental::compare_rects`, 0.01 px).

use super::*;
use crate::incremental_layout::{set_count_only_for_test, set_mode_for_test, Mode};
use crate::script_net_tests::serve_routes;

/// Resets the thread's flag overrides when a test ends, also on a panic.
struct ModeGuard;

impl ModeGuard {
    fn set(mode: Mode) -> Self {
        set_mode_for_test(Some(mode));
        ModeGuard
    }
}

impl Drop for ModeGuard {
    fn drop(&mut self) {
        set_mode_for_test(None);
        set_count_only_for_test(false);
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn stats(engine: &Engine, view: EngineViewId) -> incremental_layout::Stats {
    engine
        .views
        .get(&view)
        .expect("view")
        .lay
        .incremental_stats
        .borrow()
        .clone()
}

fn laid(engine: &Engine, view: EngineViewId) -> Rc<LayoutBox> {
    engine
        .views
        .get(&view)
        .expect("view")
        .layout
        .clone()
        .expect("laid out")
}

/// 600 sections of ten elements each (6000 elements, plus the article),
/// and a timer, past the load's horizon, that rewrites the text of one
/// paragraph near the top so it wraps to many more lines.
fn article() -> String {
    let mut body = String::new();
    for i in 0..600 {
        let target = if i == 10 { " id=t" } else { "" };
        body.push_str(&format!(
            "<section><h2>Section {i}</h2><p{target}>Lorem ipsum <b>dolor</b> sit <i>amet</i>, item {i}.</p>\
             <p>Second <span>paragraph</span> <em>text</em> {i}.</p><div class=meta><small>meta {i}</small></div></section>"
        ));
    }
    format!(
        "<html><head><style>body{{margin:8px;font-size:16px}} p{{margin:0 0 12px}} \
         h2{{margin:16px 0 8px;font-size:20px}} .meta{{color:gray}}</style></head><body><article>{body}</article>\
         <script>setTimeout(function () {{ document.getElementById('t').firstChild.data = \
         'This paragraph now says a great deal more than it did, '.repeat(12); }}, 60000);</script></body></html>"
    )
}

/// Load the article under `mode`, run the timer, and return the boxes laid
/// out while the timer's relayout ran, the view's stats and its tree.
fn timer_run(mode: Mode) -> (u64, incremental_layout::Stats, Rc<LayoutBox>) {
    let _mode = ModeGuard::set(mode);
    let server = serve_routes(vec![("/", "text/html", article())]);
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds {
            x: 0,
            y: 0,
            width: 800,
            height: 600,
        })
        .expect("view");
    let rt = runtime();
    let url = Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap();
    rt.block_on(engine.load_url(view, url)).expect("load_url");
    let before = rustkit_layout::layout_visits();
    let pump = rt.block_on(engine.pump_live(view, 60_000));
    let visits = rustkit_layout::layout_visits() - before;
    assert!(pump.timers_ran > 0, "the timer ran");
    let text = engine
        .execute_script(view, "document.getElementById('t').firstChild.data.length")
        .expect("script");
    assert!(text.contains("660"), "the paragraph was rewritten: {text}");
    (visits, stats(&engine, view), laid(&engine, view))
}

#[test]
fn a_timer_rewriting_one_paragraph_lays_out_only_that_paragraph() {
    let (full_visits, _, full_tree) = timer_run(Mode::Off);
    let (visits, stats, tree) = timer_run(Mode::On);
    eprintln!(
        "6000-element article, one paragraph rewritten by a timer: full layout visits {full_visits}, \
         incremental {visits} ({}; moved {})",
        stats.last, stats.last_moved
    );
    assert!(
        full_visits > 6000,
        "the full pass lays out every box: {full_visits}"
    );
    assert!(
        stats.last.starts_with("relaid"),
        "the timer's relayout took the incremental path: {stats:?}"
    );
    assert!(
        visits < 32,
        "O(depth + changed subtree) boxes laid out, not {visits}"
    );
    assert!(
        stats.last_moved > 5000,
        "what follows the paragraph moved: {}",
        stats.last_moved
    );
    let (count, shown) = rustkit_layout::incremental::compare_rects(&tree, &full_tree, 8);
    assert_eq!(count, 0, "the incremental tree is the full one: {shown:?}");
}

#[test]
fn verify_mode_agrees_on_the_timer_article() {
    // Verify paints the full tree and panics (cfg(test)) on any difference.
    let (_, stats, _) = timer_run(Mode::Verify);
    assert_eq!(stats.mismatches, 0);
    assert!(stats.last.starts_with("relaid"), "{stats:?}");
}

/// Load `html` under `mode`, rewrite the text of `#t`'s first text node with
/// `script`, and return what the incremental path did.
fn mutate(mode: Mode, html: &str, script: &str) -> incremental_layout::Stats {
    let _mode = ModeGuard::set(mode);
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds {
            x: 0,
            y: 0,
            width: 400,
            height: 300,
        })
        .expect("view");
    engine.load_html(view, html).expect("load_html");
    engine.execute_script(view, script).expect("script");
    stats(&engine, view)
}

const GROW: &str =
    "document.getElementById('t').firstChild.data = 'many more words in this text now '.repeat(8);";

fn last_fallback(stats: &incremental_layout::Stats) -> &str {
    stats
        .last
        .strip_prefix("fallback(")
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or(&stats.last)
}

#[test]
fn a_paragraph_under_nested_blocks_is_relaid_and_matches() {
    let html =
        "<div style='padding:4px'><div style='position:relative;left:3px'><p>a</p><p id=t>b</p>\
                <ul><li>one</li><li>two</li></ul></div><h3>after</h3><p>tail</p></div>";
    let stats = mutate(Mode::Verify, html, GROW);
    assert!(stats.last.starts_with("relaid"), "{stats:?}");
    assert_eq!(stats.mismatches, 0);
}

#[test]
fn a_change_beside_a_float_falls_back() {
    let html =
        "<div style='float:left;width:60px;height:200px'></div><p id=t>beside</p><p>after</p>";
    assert_eq!(last_fallback(&mutate(Mode::On, html, GROW)), "float_beside");
}

#[test]
fn a_change_inside_an_absolute_box_falls_back() {
    // The absolute box is hoisted to, and it cannot be laid out alone: its
    // width depends on its content and its place on its containing block.
    let html =
        "<div style='position:absolute;top:0;width:200px'><p id=t>inside</p></div><p>flow</p>";
    assert_eq!(
        last_fallback(&mutate(Mode::On, html, GROW)),
        "changed_position"
    );
}

#[test]
fn an_absolute_box_anchored_on_the_path_falls_back() {
    let html = "<div style='position:absolute;bottom:0'>pinned</div><p id=t>flow</p><p>after</p>";
    assert_eq!(
        last_fallback(&mutate(Mode::On, html, GROW)),
        "abspos_on_path"
    );
}

#[test]
fn a_margin_collapsing_through_the_changed_box_relays_out_its_parent() {
    // The div's first child is a block whose top margin collapses through
    // the div's open top edge: the div cannot be laid out alone, so its
    // padded parent is.
    let html =
        "<div style='padding:1px'><p>before</p><div id=t><p style='margin:20px 0'>first</p></div>\
                <p>after</p></div>";
    let script = "var p = document.createElement('p'); p.textContent = 'added'; \
                  document.getElementById('t').appendChild(p);";
    verified(html, script);
}

#[test]
fn a_change_whose_root_would_be_the_body_falls_back() {
    let html = "<p>before</p><div id=t><p style='margin:20px 0'>first</p></div><p>after</p>";
    let script = "var p = document.createElement('p'); p.textContent = 'added'; \
                  document.getElementById('t').appendChild(p);";
    assert_eq!(last_fallback(&mutate(Mode::On, html, script)), "body_box");
}

/// Stage 2: what the incremental path did, under `verify` (the full layout
/// is painted and every rect compared; a difference panics).
fn verified(html: &str, script: &str) -> incremental_layout::Stats {
    let stats = mutate(Mode::Verify, html, script);
    assert_eq!(stats.mismatches, 0, "{stats:?}");
    assert!(
        stats.last.starts_with("relaid"),
        "took the incremental path: {stats:?}"
    );
    stats
}

#[test]
fn a_change_inside_a_flex_item_relays_out_the_flex_container() {
    let html = "<div><div style='display:flex;gap:4px'><p id=t>item</p><p>other</p></div><p>after</p></div>";
    let stats = verified(html, GROW);
    assert!(
        stats.hoists >= 1,
        "hoisted to the flex container: {stats:?}"
    );
}

#[test]
fn a_change_inside_a_grid_item_relays_out_the_grid_container() {
    let html = "<div><div style='display:grid;grid-template-columns:1fr 1fr'><p id=t>cell</p><p>other</p></div>\
                <p>after</p></div>";
    let stats = verified(html, GROW);
    assert!(
        stats.hoists >= 1,
        "hoisted to the grid container: {stats:?}"
    );
}

#[test]
fn a_set_height_ancestor_stops_the_delta() {
    let html =
        "<div><div style='height:120px;overflow:hidden'><p id=t>x</p><p>y</p></div><p>after</p>\
                <p>more</p><p>still more</p></div>";
    let stats = verified(html, GROW);
    assert_eq!(stats.bounded, 1, "{stats:?}");
    // Only the paragraph after the changed one, inside the boundary, moved.
    assert!(
        stats.last_moved <= 2,
        "boxes after the boundary stayed: {stats:?}"
    );
}

#[test]
fn a_min_height_ancestor_grows_past_its_minimum() {
    let html =
        "<div><div style='min-height:40px;padding:2px'><p id=t>x</p></div><p>after</p></div>";
    let stats = verified(html, GROW);
    assert_eq!(stats.hoists, 0, "{stats:?}");
}

#[test]
fn a_min_height_ancestor_still_at_its_minimum_falls_back() {
    let html = "<div><div style='min-height:600px'><p id=t>x</p></div><p>after</p></div>";
    assert_eq!(
        last_fallback(&mutate(Mode::On, html, GROW)),
        "ancestor_min_max"
    );
}

#[test]
fn an_overflow_hidden_ancestor_grows() {
    let html = "<div><div style='overflow:hidden'><p id=t>x</p><p>y</p></div><p>after</p></div>";
    let stats = verified(html, GROW);
    assert_eq!(stats.hoists, 0, "{stats:?}");
}

#[test]
fn two_changed_paragraphs_relay_out_both() {
    let html = "<div><p id=t>one</p><p>between</p><div><p id=u>two</p></div><p>after</p></div>";
    let script = "document.getElementById('t').firstChild.data = 'grown text that wraps '.repeat(10); \
                  document.getElementById('u').firstChild.data = 'and this one too, longer '.repeat(9);";
    let stats = verified(html, script);
    assert_eq!(stats.roots, 2, "{stats:?}");
}

/// A `data-*` write no selector reads changes no style and no box: the
/// relayout it asks for keeps the last layout without a build.
#[test]
fn a_write_nothing_reads_skips_the_build() {
    let html = "<div><p id=t>one</p><p>two</p></div>";
    let script = "document.getElementById('t').setAttribute('data-tick', '1');";
    let on = mutate(Mode::On, html, script);
    assert_eq!(on.skipped_builds, 1, "{on:?}");
    assert_eq!(on.last, "skipped_build");
    // Verify builds anyway and checks the hint was right (a wrong one panics).
    let verify = mutate(Mode::Verify, html, script);
    assert_eq!(verify.skip_violations, 0, "{verify:?}");
    assert!(verify.last.starts_with("unchanged"), "{verify:?}");
}

/// `on` takes the published tree back instead of copying it, when nothing
/// else holds it.
#[test]
fn the_published_tree_is_reused_not_copied() {
    let (_, stats, _) = timer_run(Mode::On);
    assert!(stats.reused_trees >= 1, "{stats:?}");
}

/// The websuite cases and the `fixtures/` pages, each mutated element by
/// element (the first twelve with their own text, grown then shrunk), in
/// verify mode: every relayout the incremental path takes must give the
/// full layout's rects.
#[test]
fn verify_mode_shows_no_mismatch_on_the_layout_corpus() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut pages: Vec<std::path::PathBuf> = Vec::new();
    for dir in ["websuite/cases", "fixtures"] {
        let Ok(entries) = std::fs::read_dir(root.join(dir)) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && path.join("index.html").exists() {
                pages.push(path.join("index.html"));
            } else if path.extension().is_some_and(|e| e == "html") {
                pages.push(path);
            }
        }
    }
    pages.sort();
    assert!(pages.len() >= 15, "corpus found: {}", pages.len());
    let _mode = ModeGuard::set(Mode::Verify);
    set_count_only_for_test(true);
    let (mut relaid, mut unchanged, mut mismatches, mut mutations) = (0, 0, 0, 0);
    let (mut roots, mut hoists, mut bounded, mut visits, mut reused, mut cloned) =
        (0, 0, 0, 0, 0, 0);
    let (mut skip_ok, mut skip_bad) = (0, 0);
    let full_before = rustkit_layout::layout_visits();
    let mut fallbacks: std::collections::BTreeMap<&'static str, u32> = Default::default();
    let mut bad = Vec::new();
    for page in &pages {
        let html = std::fs::read_to_string(page).expect("page");
        let mut engine = Engine::new(EngineConfig::default()).expect("engine");
        let view = engine
            .create_headless_view(Bounds {
                x: 0,
                y: 0,
                width: 800,
                height: 600,
            })
            .expect("view");
        engine.load_html(view, &html).expect("load_html");
        let found = engine
            .execute_script(
                view,
                "window.__inc = []; var all = document.body ? document.body.getElementsByTagName('*') : []; \
                 for (var i = 0; i < all.length && window.__inc.length < 12; i++) { \
                   for (var n = all[i].firstChild; n; n = n.nextSibling) { \
                     if (n.nodeType === 3 && n.data.trim().length > 0) { window.__inc.push(n); break; } } } \
                 window.__inc.length",
            )
            .expect("script");
        let count: usize = found
            .trim_start_matches("Number(")
            .trim_end_matches(')')
            .parse::<f64>()
            .map_or(0, |n| n as usize);
        if count == 0 {
            eprintln!("{}: no text elements ({found})", page.display());
        }
        for k in 0..count {
            for script in [
                format!("window.__inc[{k}].parentNode.setAttribute('data-inc', '{k}');"),
                format!("window.__inc[{k}].data = window.__inc[{k}].data + ' and then a good deal more text to wrap'.repeat(4);"),
                format!("window.__inc[{k}].data = 'x';"),
            ] {
                engine.execute_script(view, &script).expect("mutate");
                mutations += 1;
            }
        }
        let s = stats(&engine, view);
        roots += s.roots;
        hoists += s.hoists;
        bounded += s.bounded;
        visits += s.visits;
        reused += s.reused_trees;
        cloned += s.cloned_trees;
        skip_ok += s.unchanged;
        skip_bad += s.skip_violations;
        relaid += s.relaid;
        unchanged += s.unchanged;
        mismatches += s.mismatches;
        for (why, n) in &s.fallbacks {
            *fallbacks.entry(why).or_insert(0) += n;
        }
        if s.skip_violations > 0 {
            bad.push(format!(
                "{}: skip hint wrong {} ({})",
                page.strip_prefix(&root).unwrap_or(page).display(),
                s.skip_violations,
                s.last_violation
            ));
        }
        if s.mismatches > 0 {
            bad.push(format!(
                "{}: {}",
                page.strip_prefix(&root).unwrap_or(page).display(),
                s.mismatches
            ));
        }
    }
    let all_visits = rustkit_layout::layout_visits() - full_before;
    eprintln!(
        "corpus: {} pages, {mutations} mutations, relaid {relaid} (roots {roots}, hoists {hoists}, \
         bounded {bounded}), unchanged {unchanged}, mismatches {mismatches}, skip hints wrong {skip_bad}, \
         published trees reused {reused} / copied {cloned}, incremental visits {visits} of {all_visits} \
         laid out in all (verify runs both), fallbacks {fallbacks:?}",
        pages.len()
    );
    let _ = skip_ok;
    assert_eq!(skip_bad, 0, "a skip hint was wrong: {bad:?}");
    assert_eq!(mismatches, 0, "pages with mismatches: {bad:?}");
    assert!(
        relaid > 20,
        "the corpus exercised the incremental path: relaid {relaid}"
    );
}
