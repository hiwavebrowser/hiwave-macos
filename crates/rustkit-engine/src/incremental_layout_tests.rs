//! INC5 stage 1 (`docs/design/INCREMENTAL_LAYOUT_PLAN.md`): incremental
//! layout of one block box whose content changed, behind
//! `RUSTKIT_INCREMENTAL_LAYOUT`. The oracle is the full layout of the same
//! DOM (`rustkit_layout::incremental::compare_rects`, 0.01 px).

use super::*;
use crate::incremental_layout::{set_count_only_for_test, set_mode_for_test, Mode};
use crate::script_net_tests::{serve_byte_routes, serve_routes};

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

/// Prometheus's #1063 review (MED 1): an ancestor with `aspect-ratio` and
/// an auto height takes its height from its width and grows with its
/// content only past the ratio height. A paragraph growing inside it must
/// not grow it by the delta: the box is laid out whole (hoisted to).
#[test]
fn a_change_inside_an_aspect_ratio_box_relays_out_the_box() {
    // Padded, so the box's own margins stay off its children's and it can
    // be laid out alone.
    let html = "<div><div style='aspect-ratio:2/1;padding:1px'><p id=t>x</p></div><p>after</p></div>";
    let stats = verified(html, GROW);
    assert!(stats.hoists >= 1, "hoisted to the aspect-ratio box: {stats:?}");
}

const AHEM: &[u8] = include_bytes!("../../rustkit-text/tests/fixtures/Ahem.ttf");

/// A web font declared in the page, fetched after the first layout (every
/// glyph of Ahem is a 20px square here, so its lines measure unlike the
/// fallback's), and a paragraph to rewrite once it has landed.
const FONT_PAGE: &str = "<!doctype html><html><head><style>\
    @font-face { font-family: IncFace; src: url(/Ahem.ttf) format('truetype'); } \
    body { margin: 8px; font: 20px/20px IncFace, Helvetica; } p { margin: 0 0 10px; padding: 1px 0 }\
    </style></head><body><div><p>XXXXX xx XXXX</p><p id=t>xx XXX</p><p>after</p></div></body></html>";

/// Prometheus's #1063 review (MED 2), batch 59's cnn finding: a web font
/// finishing its load changes text metrics without changing the box tree.
/// The retained layout must not stand: the relayout the load asks for
/// measures text afresh (verify: 0 mismatches, panics otherwise), and a
/// change after the face landed is laid out incrementally in it.
#[test]
fn a_web_font_landing_after_layout_measures_text_afresh() {
    let _mode = ModeGuard::set(Mode::Verify);
    let server = serve_byte_routes(vec![
        ("/", "text/html", FONT_PAGE.as_bytes().to_vec()),
        ("/Ahem.ttf", "font/ttf", AHEM.to_vec()),
    ]);
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds {
            x: 0,
            y: 0,
            width: 400,
            height: 300,
        })
        .expect("view");
    let rt = runtime();
    let url = Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap();
    rt.block_on(engine.load_url(view, url)).expect("load_url");
    let loaded = stats(&engine, view);
    assert_eq!(loaded.mismatches, 0, "{loaded:?}");
    assert_eq!(loaded.last, "fallback(key_changed)", "the face landing moved the key: {loaded:?}");
    // The first line is in Ahem: 13 characters of 20px.
    let tree = laid(&engine, view);
    let mut widths = Vec::new();
    fn text_widths(b: &LayoutBox, out: &mut Vec<f32>) {
        if matches!(b.box_type, BoxType::Text(_)) {
            out.push(b.dimensions.content.width);
        }
        b.children.iter().for_each(|c| text_widths(c, out));
    }
    text_widths(&tree, &mut widths);
    assert!(
        widths.first().is_some_and(|w| (w - 260.0).abs() < 0.5),
        "the page is measured in the web font: {widths:?} ({loaded:?})"
    );
    engine
        .execute_script(view, "document.getElementById('t').firstChild.data = 'XX xxx XXX '.repeat(6);")
        .expect("script");
    let after = stats(&engine, view);
    assert_eq!(after.mismatches, 0, "{after:?}");
    assert!(after.last.starts_with("relaid"), "{after:?}");
}

/// Page shapes common on real sites (batch 59's A/B fallbacks were in
/// boxes like these): cards with absolute badges and overlays, an inset
/// background layer, a header with an absolute dropdown, a tooltip after
/// the text, an image in a line, percentage heights, a list that grows,
/// a fixed header and a flex shell. Synthetic, so a proxy for the A/B.
const SHAPES: &[(&str, &str)] = &[
    (
        "card badges",
        "<style>.card{position:relative;padding:8px;margin:8px 0;border:1px solid #ccc}\
         .badge{position:absolute;top:4px;right:4px;width:30px;height:12px}</style>\
         <main><div class=card><span class=badge>new</span><h3>One</h3><p>first card</p></div>\
         <div class=card><span class=badge>hot</span><h3>Two</h3><p>second card</p></div>\
         <div class=card><h3>Three</h3><p>third card</p></div></main>",
    ),
    (
        "card overlay",
        "<style>.tile{position:relative;padding:4px}.cap{position:absolute;left:0;right:0;bottom:0;height:16px}</style>\
         <section><div class=tile><p>tile text one</p><div class=cap>caption</div></div>\
         <div class=tile><p>tile text two</p><div class=cap>caption</div></div><p>after</p></section>",
    ),
    (
        "inset layer",
        "<style>.hero{position:relative;padding:20px}.bg{position:absolute;inset:0;z-index:-1}</style>\
         <div><div class=hero><div class=bg></div><h1>Title</h1><p>hero text</p></div><p>below</p></div>",
    ),
    (
        "header dropdown",
        "<style>header{position:relative;padding:4px}.menu{position:absolute;top:100%;left:0;width:200px}</style>\
         <header><nav>Home News Sport</nav><div class=menu><p>a</p><p>b</p></div></header>\
         <main><article><p>story text one</p><p>story text two</p></article></main>",
    ),
    (
        "tooltip after text",
        "<div style='position:relative;padding:2px'><p>term</p><div style='position:absolute'>tip</div><p>more</p></div><p>end</p>",
    ),
    (
        "image in a line",
        "<div style='padding:1px'><p>text <img width=10 height=10> more text</p><p>next</p></div><p>end</p>",
    ),
    (
        "percent heights",
        "<div style='padding:1px'><div style='height:50%'><p>half</p></div><div style='min-height:10%'><p>min</p></div></div>",
    ),
    (
        "growing list",
        "<div style='padding:1px'><ul><li>one</li><li>two</li></ul><p>after the list</p></div>",
    ),
    (
        "fixed header",
        "<div style='position:fixed;top:0;left:0;right:0;height:20px'>bar</div>\
         <div style='padding-top:24px'><p>body text one</p><p>body text two</p></div>",
    ),
    (
        "flex shell",
        "<div style='display:flex'><aside style='width:100px'>side</aside>\
         <main style='flex:1'><div style='padding:1px'><p>column text</p><p>more column</p></div></main></div>",
    ),
];

/// Load each shape, mutate its text nodes (grow, shrink), its images'
/// heights and its lists, in verify mode; sum what the incremental path did.
fn shapes_run() -> (incremental_layout::Stats, u32) {
    let _mode = ModeGuard::set(Mode::Verify);
    set_count_only_for_test(true);
    let mut sum = incremental_layout::Stats::default();
    let mut mutations = 0;
    for (name, body) in SHAPES {
        let mut engine = Engine::new(EngineConfig::default()).expect("engine");
        let view = engine
            .create_headless_view(Bounds {
                x: 0,
                y: 0,
                width: 800,
                height: 600,
            })
            .expect("view");
        engine
            .load_html(view, &format!("<!doctype html><html><body>{body}</body></html>"))
            .expect("load_html");
        let found = engine
            .execute_script(
                view,
                "window.__inc = []; var all = document.body.getElementsByTagName('*'); \
                 for (var i = 0; i < all.length; i++) { \
                   for (var n = all[i].firstChild; n; n = n.nextSibling) { \
                     if (n.nodeType === 3 && n.data.trim().length > 0) { window.__inc.push(n); break; } } } \
                 window.__inc.length",
            )
            .expect("script");
        let count = found
            .trim_start_matches("Number(")
            .trim_end_matches(')')
            .parse::<f64>()
            .map_or(0, |n| n as usize);
        let mut scripts = Vec::new();
        for k in 0..count {
            scripts.push(format!("window.__inc[{k}].data = window.__inc[{k}].data + ' with a good deal more text to wrap'.repeat(5);"));
            scripts.push(format!("window.__inc[{k}].data = 'x';"));
        }
        scripts.push("var im = document.querySelector('img'); if (im) im.setAttribute('height', '40');".into());
        scripts.push("var ul = document.querySelector('ul'); if (ul) { var li = document.createElement('li'); li.textContent = 'three'; ul.appendChild(li); }".into());
        for script in &scripts {
            engine.execute_script(view, script).expect("mutate");
            mutations += 1;
        }
        let s = stats(&engine, view);
        eprintln!("shape {name}: relaid {} unchanged {} fallbacks {:?} mismatches {}", s.relaid, s.unchanged, s.fallbacks, s.mismatches);
        sum.attempts += s.attempts;
        sum.relaid += s.relaid;
        sum.unchanged += s.unchanged;
        sum.mismatches += s.mismatches;
        for (why, n) in &s.fallbacks {
            *sum.fallbacks.entry(why).or_insert(0) += n;
        }
    }
    (sum, mutations)
}

/// Stage 4 (absolute boxes): every relayout over the real-site shapes but
/// the first layouts, the fixed header's and the percentage heights' (the
/// root would be the body) takes the incremental path, with 0 mismatches.
#[test]
fn real_site_shapes_take_the_incremental_path() {
    let (s, mutations) = shapes_run();
    let first = s.fallbacks.get("no_retained").copied().unwrap_or(0);
    let relayouts = s.attempts - first;
    let share = (s.relaid + s.unchanged) as f32 / relayouts.max(1) as f32;
    eprintln!(
        "real-site shapes: {} shapes, {mutations} mutations, {} relayouts after the first layouts, \
         relaid {} unchanged {} ({:.0}% incremental), mismatches {}, fallbacks {:?}",
        SHAPES.len(),
        relayouts,
        s.relaid,
        s.unchanged,
        share * 100.0,
        s.mismatches,
        s.fallbacks
    );
    assert_eq!(s.mismatches, 0);
    assert_eq!(s.fallbacks.get("abspos_on_path"), None, "{:?}", s.fallbacks);
}

/// A card's absolute badge (`top`/`right`) sits in the card the change
/// grows: it is re-anchored, not a reason to lay the page out.
#[test]
fn a_change_in_a_card_with_an_absolute_badge_is_relaid() {
    let html = "<div><div style='position:relative;padding:4px'><span style='position:absolute;top:2px;right:2px'>new</span>\
                <p id=t>card</p></div><p>after</p></div>";
    verified(html, GROW);
}

/// A caption anchored to the bottom of the box that grows moves with its
/// bottom edge.
#[test]
fn a_bottom_anchored_overlay_follows_the_growing_box() {
    let html = "<div><div style='position:relative;padding:4px'><p id=t>tile</p>\
                <div style='position:absolute;left:0;right:0;bottom:0;height:10px'>cap</div></div><p>after</p></div>";
    verified(html, GROW);
}

/// An `inset: 0` layer stretches with the box that grows.
#[test]
fn an_inset_layer_stretches_with_the_growing_box() {
    let html = "<div><div style='position:relative;padding:4px'><div style='position:absolute;inset:0'></div>\
                <p id=t>hero</p></div><p>after</p></div>";
    verified(html, GROW);
}

/// An absolute box after the change with `top` set does not move with the
/// flow; one at its static position does.
#[test]
fn absolute_boxes_after_the_change_keep_or_follow_their_anchor() {
    let html = "<div><div style='position:relative;padding:4px'><p id=t>text</p>\
                <div style='position:absolute;top:3px;left:0'>pinned</div><div style='position:absolute'>static</div>\
                <p>more</p></div><p>after</p></div>";
    verified(html, GROW);
}

/// Stage 3 (margin chains): a wrapper with open edges whose first and last
/// children's margins collapse through it is laid out alone when those
/// margin chains are what they were; no padded ancestor is needed.
#[test]
fn a_wrapper_whose_edge_margins_stay_is_relaid_alone() {
    let html = "<div><div><p>before</p><div id=t><p style='margin:20px 0'>first</p>\
                <p style='margin:20px 0'>last</p></div><p>after</p></div></div>";
    let script = "var t = document.getElementById('t'); var p = document.createElement('p'); \
                  p.textContent = 'added in the middle'; t.insertBefore(p, t.lastElementChild);";
    let stats = verified(html, script);
    assert_eq!(stats.hoists, 0, "{stats:?}");
}

/// When the last child changes, the margin that collapses through the
/// wrapper's bottom edge may change: that still falls back.
#[test]
fn a_wrapper_whose_last_margin_changes_falls_back() {
    let html = "<div><div><p>before</p><div id=t><p style='margin:20px 0'>first</p></div><p>after</p></div></div>";
    let script = "var p = document.createElement('p'); p.style.margin = '5px 0'; p.textContent = 'added'; \
                  document.getElementById('t').appendChild(p);";
    let on = mutate(Mode::On, html, script);
    assert!(last_fallback(&on).starts_with("margin_collapse"), "{on:?}");
    let verify = mutate(Mode::Verify, html, script);
    assert_eq!(verify.mismatches, 0, "{verify:?}");
}
