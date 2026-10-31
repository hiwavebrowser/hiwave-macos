//! The census must not report a property it did not measure, and a report it
//! prints must say which of parse, computed and effect each property reached.

use css_census::*;
use std::collections::BTreeSet;

fn prop(name: &str, context: &str, values: &[&str]) -> PropSpec {
    PropSpec {
        name: name.into(),
        context: context.into(),
        base: String::new(),
        values: values.iter().map(|v| v.to_string()).collect(),
        nonvisual: false,
        pseudo: None,
    }
}

fn frame(w: usize, h: usize, rgb: [u8; 3]) -> Frame {
    Frame { width: w, height: h, rgb: rgb.iter().cycle().take(w * h * 3).copied().collect() }
}

fn paint(f: &mut Frame, x0: usize, y0: usize, x1: usize, y1: usize, rgb: [u8; 3]) {
    for y in y0..y1 {
        for x in x0..x1 {
            let i = (y * f.width + x) * 3;
            f.rgb[i..i + 3].copy_from_slice(&rgb);
        }
    }
}

fn read(computed: &str) -> PageRead {
    PageRead { computed: computed.into(), rects: vec![("p".into(), [0.0, 0.0, 100.0, 50.0])] }
}

/// One value's four captures, all the same flat page until a test paints one.
fn measured(chrome: &str, rust: &str) -> Measure {
    let white = frame(64, 64, [255, 255, 255]);
    Measure {
        chrome_value: Capture { read: Some(read(chrome)), frame: Some(white.clone()) },
        chrome_control: Capture { read: Some(read("control")), frame: Some(white.clone()) },
        rust_value: Capture { read: Some(read(rust)), frame: Some(white.clone()) },
        rust_control: Capture { read: Some(read("control")), frame: Some(white) },
        arm: true,
        nonvisual: false,
    }
}

// ---- the checked-in list ----

#[test]
fn the_checked_in_list_is_about_250_distinct_properties_on_known_contexts() {
    let spec = load_spec(include_str!("../properties.json")).expect("the list parses");
    let names: BTreeSet<&str> = spec.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names.len(), spec.len(), "a property is listed twice");
    assert!(spec.len() >= 240, "only {} properties", spec.len());
    for p in &spec {
        assert!(context_exists(&p.context), "{}: no context {}", p.name, p.context);
        assert!((1..=3).contains(&p.values.len()), "{}: wants 1 to 3 values", p.name);
    }
}

#[test]
fn the_list_covers_the_baseline_computed_style_properties() {
    let spec = load_spec(include_str!("../properties.json")).unwrap();
    let names: BTreeSet<&str> = spec.iter().map(|p| p.name.as_str()).collect();
    let baseline: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../baselines/chrome-148/websuite/article-typography/computed-styles.json"
    ))
    .unwrap();
    let styles = baseline["elements"][0]["styles"].as_object().unwrap();
    assert!(styles.len() > 50);
    for name in styles.keys() {
        assert!(names.contains(name.as_str()), "baseline property {name} is not listed");
    }
}

#[test]
fn a_load_refuses_a_list_entry_with_no_values() {
    assert!(load_spec(r#"[{"name":"width","context":"box","values":[]}]"#).is_err());
}

// ---- the fixture ----

#[test]
fn a_fixture_declares_the_value_and_reads_it_back() {
    let html = fixture_html(&prop("margin-left", "box", &["12px"]), Some("12px"));
    assert!(html.contains("margin-left:12px"), "{html}");
    assert!(html.contains("getPropertyValue(\"margin-left\")"), "{html}");
    assert!(html.contains("CENSUS:"), "{html}");
}

#[test]
fn a_control_fixture_is_the_same_page_without_the_declaration() {
    let p = prop("margin-left", "box", &["12px"]);
    let control = fixture_html(&p, None);
    assert!(!control.contains("margin-left:12px"), "{control}");
    assert_eq!(control, fixture_html(&p, Some("12px")).replace("margin-left:12px", ""));
}

#[test]
fn a_base_declaration_is_in_both_arms() {
    let mut p = prop("border-top-color", "box", &["red"]);
    p.base = "border:6px solid #060".into();
    assert!(fixture_html(&p, None).contains("border:6px solid #060"));
    assert!(fixture_html(&p, Some("red")).contains("border:6px solid #060"));
}

#[test]
fn a_pseudo_property_is_declared_on_and_read_from_the_pseudo() {
    let mut p = prop("content", "text", &["\"HH\""]);
    p.pseudo = Some("::before".into());
    let html = fixture_html(&p, Some("\"HH\""));
    assert!(html.contains("#p::before{content:\"HH\""), "{html}");
    assert!(html.contains("getComputedStyle(document.getElementById(\"p\"),\"::before\")"), "{html}");
}

// ---- reading the captures ----

#[test]
fn the_page_read_comes_from_the_census_console_line_in_both_shapes() {
    let line = r#"CENSUS:{"c":"12px","r":{"p":[1,2,3,4]}}"#;
    let got = parse_read(line).expect("a line");
    assert_eq!(got.computed, "12px");
    assert_eq!(got.rects, vec![("p".to_string(), [1.0, 2.0, 3.0, 4.0])]);
    assert!(parse_read("hello").is_none());
}

#[test]
fn the_event_log_gives_up_the_census_line() {
    let log = concat!(
        r#"{"seq":1,"type":"load"}"#,
        "\n",
        r#"{"seq":2,"type":"console","level":"log","message":"CENSUS:{\"c\":\"x\",\"r\":{}}"}"#,
        "\n"
    );
    assert_eq!(read_from_event_log(log).expect("a read").computed, "x");
    assert!(read_from_event_log(r#"{"seq":1,"type":"load"}"#).is_none());
}

#[test]
fn a_ppm_is_read_with_its_comment_and_size() {
    let mut bytes = b"P6\n# c\n2 1\n255\n".to_vec();
    bytes.extend_from_slice(&[1, 2, 3, 4, 5, 6]);
    let f = read_ppm(&bytes).expect("a frame");
    assert_eq!((f.width, f.height), (2, 1));
    assert_eq!(f.rgb, vec![1, 2, 3, 4, 5, 6]);
    assert!(read_ppm(b"P5\n1 1\n255\nx").is_none());
}

// ---- the probes ----

#[test]
fn a_probe_forgives_one_pixel_of_seat_and_a_little_colour() {
    let a = frame(64, 64, [255, 255, 255]);
    let mut b = a.clone();
    assert_eq!(probe_diff(&a, &b), 0);
    // A lone off pixel with matching neighbours is antialiasing, not a change.
    paint(&mut b, 8, 8, 9, 9, [0, 0, 0]);
    assert_eq!(probe_diff(&a, &b), 0);
    let near = frame(64, 64, [250, 250, 250]);
    assert_eq!(probe_diff(&a, &near), 0);
}

#[test]
fn a_probe_sees_a_painted_block() {
    let a = frame(64, 64, [255, 255, 255]);
    let mut b = a.clone();
    paint(&mut b, 8, 8, 32, 32, [200, 0, 0]);
    assert!(probe_diff(&a, &b) >= 9, "{}", probe_diff(&a, &b));
}

#[test]
fn frames_of_different_sizes_differ_on_every_probe() {
    let a = frame(64, 64, [255, 255, 255]);
    let b = frame(32, 32, [255, 255, 255]);
    assert!(probe_diff(&a, &b) > 100);
}

// ---- the verdict on one value ----

#[test]
fn the_engine_painting_what_chrome_painted_is_effect() {
    let mut m = measured("12px", "12px");
    paint(m.chrome_value.frame.as_mut().unwrap(), 8, 8, 32, 32, [200, 0, 0]);
    paint(m.rust_value.frame.as_mut().unwrap(), 8, 8, 32, 32, [200, 0, 0]);
    let v = judge(&m);
    assert_eq!(v.effect, Effect::Match);
    assert!(v.computed);
    assert!(v.parse);
}

#[test]
fn the_engine_ignoring_a_value_chrome_paints_fails_effect_even_with_an_arm() {
    let mut m = measured("12px", "12px");
    paint(m.chrome_value.frame.as_mut().unwrap(), 8, 8, 32, 32, [200, 0, 0]);
    assert_eq!(judge(&m).effect, Effect::Mismatch);
}

#[test]
fn a_geometry_miss_fails_effect_with_matching_pixels() {
    let mut m = measured("12px", "12px");
    paint(m.chrome_value.frame.as_mut().unwrap(), 8, 8, 32, 32, [200, 0, 0]);
    paint(m.rust_value.frame.as_mut().unwrap(), 8, 8, 32, 32, [200, 0, 0]);
    m.rust_value.read.as_mut().unwrap().rects[0].1 = [0.0, 0.0, 140.0, 50.0];
    assert_eq!(judge(&m).effect, Effect::Mismatch);
}

#[test]
fn a_value_that_changes_nothing_in_chrome_is_not_an_effect_pass() {
    // Nothing moved in Chrome, so the fixture proved nothing about the engine.
    assert_eq!(judge(&measured("12px", "12px")).effect, Effect::Unexercised);
}

#[test]
fn a_geometry_change_alone_exercises_a_value() {
    let mut m = measured("12px", "12px");
    m.chrome_value.read.as_mut().unwrap().rects[0].1 = [0.0, 0.0, 140.0, 50.0];
    m.rust_value.read.as_mut().unwrap().rects[0].1 = [0.0, 0.0, 140.0, 50.0];
    assert_eq!(judge(&m).effect, Effect::Match);
}

#[test]
fn control_noise_is_credited_but_cannot_hide_the_signal() {
    let mut m = measured("12px", "12px");
    // The engine's control is already off Chrome's on one region.
    paint(m.rust_control.frame.as_mut().unwrap(), 40, 40, 60, 60, [0, 0, 200]);
    paint(m.rust_value.frame.as_mut().unwrap(), 40, 40, 60, 60, [0, 0, 200]);
    paint(m.chrome_value.frame.as_mut().unwrap(), 8, 8, 32, 32, [200, 0, 0]);
    assert_eq!(judge(&m).effect, Effect::Mismatch, "noise must not excuse a missing signal");
    paint(m.rust_value.frame.as_mut().unwrap(), 8, 8, 32, 32, [200, 0, 0]);
    assert_eq!(judge(&m).effect, Effect::Match, "the same noise on a real effect passes");
}

#[test]
fn a_nonvisual_property_has_no_effect_verdict() {
    let mut m = measured("pointer", "pointer");
    m.nonvisual = true;
    paint(m.chrome_value.frame.as_mut().unwrap(), 8, 8, 32, 32, [200, 0, 0]);
    assert_eq!(judge(&m).effect, Effect::NotVisual);
}

#[test]
fn a_failed_capture_is_said_and_never_passes() {
    let mut m = measured("12px", "12px");
    m.rust_value = Capture { read: None, frame: None };
    let v = judge(&m);
    assert_eq!(v.effect, Effect::NoCapture);
    assert!(!v.computed);
}

#[test]
fn the_computed_read_is_compared_with_numeric_slack_but_never_blank() {
    assert!(computed_matches("12px", "12px"));
    assert!(computed_matches("12.004px", "12px"));
    assert!(computed_matches("rgb(1, 2, 3)", "rgb(1,  2, 3)"));
    assert!(!computed_matches("12px", "13px"));
    assert!(!computed_matches("rgb(1, 2, 3)", "rgba(1, 2, 3, 1)"));
    assert!(!computed_matches("none", ""));
    assert!(!computed_matches("", ""), "an empty answer proves nothing");
}

#[test]
fn parse_is_the_arm_or_any_change_the_engine_made() {
    let mut m = measured("x", "");
    m.arm = false;
    assert!(!judge(&m).parse, "no arm and no change");
    m.rust_value.read.as_mut().unwrap().computed = "y".into();
    assert!(judge(&m).parse, "the computed read moved");
    let mut m = measured("x", "");
    m.arm = false;
    paint(m.rust_value.frame.as_mut().unwrap(), 8, 8, 32, 32, [200, 0, 0]);
    assert!(judge(&m).parse, "the pixels moved");
}

// ---- the verdict on a property ----

fn verdict(parse: bool, computed: bool, effect: Effect) -> Verdict {
    Verdict { parse, computed, effect, computed_read_vacuous: false }
}

#[test]
fn a_property_is_its_worst_value_on_the_ladder() {
    use Class::*;
    let good = verdict(true, true, Effect::Match);
    assert_eq!(class_of(&[good.clone()]), Effect_);
    assert_eq!(class_of(&[good.clone(), verdict(true, true, Effect::Mismatch)]), Computed);
    assert_eq!(class_of(&[good.clone(), verdict(true, false, Effect::Mismatch)]), Parse);
    assert_eq!(class_of(&[good, verdict(false, false, Effect::Mismatch)]), Unsupported);
}

#[test]
fn an_effect_that_matches_with_an_unread_computed_value_is_still_effect() {
    assert_eq!(class_of(&[verdict(true, false, Effect::Match)]), Class::Effect_);
}

#[test]
fn nothing_measured_never_reaches_effect() {
    assert_eq!(class_of(&[verdict(true, true, Effect::Unexercised)]), Class::Computed);
    assert_eq!(class_of(&[verdict(true, true, Effect::NotVisual)]), Class::Computed);
    assert_eq!(class_of(&[verdict(true, false, Effect::NotVisual)]), Class::Parse);
    assert_eq!(class_of(&[verdict(true, true, Effect::NoCapture)]), Class::Computed);
    assert_eq!(class_of(&[]), Class::Unsupported);
}

#[test]
fn the_red_list_holds_what_does_not_work_and_not_what_cannot_be_seen() {
    assert!(is_red(Class::Unsupported, &[verdict(false, false, Effect::Mismatch)]));
    assert!(is_red(Class::Parse, &[verdict(true, false, Effect::Mismatch)]));
    assert!(is_red(Class::Computed, &[verdict(true, true, Effect::Mismatch)]));
    assert!(!is_red(Class::Computed, &[verdict(true, true, Effect::NotVisual)]));
    assert!(!is_red(Class::Effect_, &[verdict(true, true, Effect::Match)]));
    // Non-visual and read wrong: the page can see it, so it is red.
    assert!(is_red(Class::Parse, &[verdict(true, false, Effect::NotVisual)]));
}

// ---- the arms ----

const ENGINE: &str = r#"
fn logical_to_physical(p: &str) -> Option<LogicalMapping> {
    Some(match p {
        "margin-inline-start" => LogicalMapping::Side("margin-left"),
        "margin-inline" => LogicalMapping::Pair("margin-left", "margin-right"),
        _ => return None,
    })
}
impl X {
    fn apply_style_property(&self, style: &mut ComputedStyle, property: &str, value: &str) {
        match property {
            "display" => { match value { "flex" => {} , _ => {} } }
            "width" | "height" => {}
            "position" => {}
            "margin-top" => {}
            "padding-top" => {}
            "color" => {}
            _ => {}
        }
    }
    fn after(&self) { match x { "not-an-arm" => {} } }
}
"#;

#[test]
fn arms_come_from_the_match_and_the_logical_map_only() {
    let arms = extract_arms(ENGINE);
    for name in ["display", "width", "height", "color", "margin-inline-start", "margin-inline"] {
        assert!(arms.contains(name), "{name} in {arms:?}");
    }
    assert!(!arms.contains("flex"), "a value is not an arm");
    assert!(!arms.contains("not-an-arm"), "another function is not an arm");
}

#[test]
fn the_real_engine_yields_a_believable_arm_set() {
    let src = include_str!("../../../../../crates/rustkit-engine/src/lib.rs");
    let arms = check_arms(extract_arms(src)).expect("a believable set");
    assert!(arms.contains("margin-inline-start"));
}

#[test]
fn an_implausible_arm_set_is_a_refusal_not_a_clean_board() {
    assert!(check_arms(BTreeSet::new()).is_err());
    assert!(check_arms(["color".to_string()].into_iter().collect()).is_err());
}

// ---- the order ----

#[test]
fn prevalence_counts_only_loaded_sites_that_use_the_property() {
    let census = serde_json::json!({ "results": [
        { "status": "OK", "data": { "objectFit": { "by_value": { "cover": 2 } },
            "stacking": { "by_reason": { "transform": 3, "opacity<1": 0 } } } },
        { "status": "OK", "data": { "objectFit": { "by_value": {} },
            "stacking": { "by_reason": { "transform": 1 } } } },
        { "status": "BOTWALL", "data": { "objectFit": { "by_value": { "cover": 9 } },
            "stacking": { "by_reason": { "transform": 9 } } } },
    ]});
    let p = TopSites::from_census(&census);
    assert_eq!(p.loaded, 2);
    assert_eq!(p.sites("transform"), Some(2));
    assert_eq!(p.sites("object-fit"), Some(1));
    assert_eq!(p.sites("opacity"), Some(0));
    assert_eq!(p.sites("letter-spacing"), None, "not measured is not zero");
}

#[test]
fn the_corpus_count_finds_authored_declarations_not_longer_names() {
    let n = corpus_count("<style>a{margin-top:1px;top:0}b{ top : 1px }</style><p style=\"top:2px\">", "top");
    assert_eq!(n, 3);
}

// ---- the report ----

fn row(name: &str, sites: Option<usize>, corpus: usize, class: Class, red: bool) -> Row {
    Row {
        name: name.into(),
        context: "box".into(),
        sites,
        corpus,
        class,
        red,
        values: vec![ValueRow {
            value: "12px".into(),
            parse: class != Class::Unsupported,
            computed: class == Class::Computed,
            effect: if red { Effect::Mismatch } else { Effect::Match },
            chrome_computed: "12px".into(),
            engine_computed: String::new(),
            probes_off: 0,
        }],
    }
}

fn meta() -> Meta {
    Meta {
        date: "2026-10-31".into(),
        develop_sha: "abc".into(),
        oracle: "143.0.7499.4".into(),
        sites_loaded: 67,
        contexts_off: vec![],
    }
}

#[test]
fn rows_order_by_sites_then_corpus_then_name_and_unmeasured_sites_go_last() {
    let mut rows = vec![
        row("b", None, 9, Class::Effect_, false),
        row("a", None, 9, Class::Effect_, false),
        row("c", Some(1), 0, Class::Effect_, false),
        row("d", Some(5), 0, Class::Effect_, false),
        row("e", None, 40, Class::Effect_, false),
    ];
    sort_rows(&mut rows);
    let got: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(got, ["d", "c", "e", "a", "b"]);
}

#[test]
fn the_report_counts_every_class_and_names_the_red_list() {
    let rows = vec![
        row("width", Some(5), 1, Class::Effect_, false),
        row("float", Some(4), 1, Class::Computed, true),
        row("zoom", None, 0, Class::Unsupported, true),
    ];
    let md = render_markdown(&meta(), &rows);
    assert!(md.contains("143.0.7499.4"));
    assert!(md.contains("| effect | 1 |"), "{md}");
    assert!(md.contains("| computed | 1 |"), "{md}");
    assert!(md.contains("| unsupported | 1 |"), "{md}");
    let red = md.find("## Red list").expect("a red list");
    let all = md.find("## All properties").expect("the whole table");
    assert!(red < all);
    assert!(md[red..all].contains("`float`") && md[red..all].contains("`zoom`"));
    assert!(!md[red..all].contains("`width`"), "green is not on the red list");
}

#[test]
fn the_report_says_what_it_did_not_measure() {
    let md = render_markdown(&meta(), &[row("width", None, 0, Class::Effect_, false)]);
    assert!(md.contains("not measured"), "{md}");
}

#[test]
fn the_report_prints_an_off_control_rather_than_hiding_it() {
    let mut m = meta();
    m.contexts_off = vec![("text".into(), 14)];
    let md = render_markdown(&m, &[]);
    assert!(md.contains("text") && md.contains("14"), "{md}");
}

#[test]
fn the_json_says_the_same_as_the_table() {
    let v = render_json(&meta(), &[row("float", Some(4), 1, Class::Computed, true)]);
    assert_eq!(v["meta"]["oracle"], "143.0.7499.4");
    assert_eq!(v["counts"]["computed"], 1);
    assert_eq!(v["red"][0], "float");
    assert_eq!(v["properties"][0]["class"], "computed");
    assert_eq!(v["properties"][0]["values"][0]["effect"], "mismatch");
}

#[test]
fn a_sentinel_check_catches_a_blind_instrument() {
    // Comparing Chrome's value against the engine's control must read as a
    // miss for a property that plainly paints; a pass means the probes see
    // nothing.
    let mut m = measured("rgb(200, 0, 0)", "rgb(200, 0, 0)");
    paint(m.chrome_value.frame.as_mut().unwrap(), 8, 8, 32, 32, [200, 0, 0]);
    paint(m.rust_value.frame.as_mut().unwrap(), 8, 8, 32, 32, [200, 0, 0]);
    assert!(sentinel_sees(&m));
    let blind = measured("rgb(200, 0, 0)", "rgb(200, 0, 0)");
    assert!(!sentinel_sees(&blind));
}
