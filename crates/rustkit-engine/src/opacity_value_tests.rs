//! `opacity` takes `<number> | <percentage>`, and `calc()` of either. The arm
//! read a bare number only, so `opacity:50%` (what Tailwind v4 writes) did
//! nothing: no fade, no stacking context, computed `1`. Expected values are
//! Chromium 143.0.7499.4 on the same pages (`trench/z/tools/c2/opacity_pct_probe.mjs`
//! on the hub).

use super::*;

/// The computed `opacity` of `#t`, and the opacity its red box paints at.
fn read(html: &str, script: &str) -> (String, f32) {
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 400, height: 300 })
        .expect("view");
    engine.load_html(view, html).expect("load");
    if !script.is_empty() {
        engine.execute_script(view, script).expect("script");
    }
    let computed = engine
        .execute_script(view, "getComputedStyle(document.getElementById('t')).opacity")
        .expect("script");
    let computed = computed.trim_start_matches("String(\"").trim_end_matches("\")").to_string();
    let list = engine.views[&view].display_list.as_ref().expect("display list");
    let red = rustkit_css::Color { r: 255, g: 0, b: 0, a: 1.0 };
    let mut stack = vec![1.0f32];
    let mut painted = 0.0;
    for command in &list.commands {
        match command {
            rustkit_layout::DisplayCommand::PushOpacity(a) => stack.push(*a),
            rustkit_layout::DisplayCommand::PopOpacity => {
                stack.pop();
            }
            rustkit_layout::DisplayCommand::SolidColor(color, _) if *color == red => {
                painted = stack.iter().product();
                break;
            }
            _ => {}
        }
    }
    (computed, painted)
}

fn page(css: &str, style: &str) -> String {
    format!(
        "<!doctype html><style>body{{margin:0}}#t{{width:100px;height:100px;background:#f00}}{css}</style>\
         <body style=\"--p:50%;--n:.5\"><div id=t style=\"{style}\"></div></body>"
    )
}

#[track_caller]
fn check(cases: &[(String, f32, f32)], script: &str) {
    let wrong: Vec<String> = cases
        .iter()
        .filter_map(|(html, computed, paint)| {
            let (got, painted) = read(html, script);
            let number = got.parse::<f32>().unwrap_or(f32::NAN);
            let same = (number - computed).abs() < 1e-4 && (painted - paint).abs() < 1e-4;
            (!same).then(|| format!("computed {got} paint {painted}, Chromium {computed} {paint}: {html}"))
        })
        .collect();
    assert!(wrong.is_empty(), "{} of {} wrong:\n{}", wrong.len(), cases.len(), wrong.join("\n"));
}

#[track_caller]
fn check_values(values: &[(&str, f32)]) {
    let cases: Vec<(String, f32, f32)> =
        values.iter().map(|&(v, want)| (page("", &format!("opacity:{v}")), want, want)).collect();
    check(&cases, "");
}

#[test]
fn a_percentage_is_a_hundredth_clamped_when_used() {
    check_values(&[
        ("50%", 0.5),
        ("99%", 0.99),
        ("100%", 1.0),
        ("0%", 0.0),
        ("150%", 1.0),
        ("-10%", 0.0),
        ("12.5%", 0.125),
        ("5e1%", 0.5),
        ("5E1%", 0.5),
        ("+50%", 0.5),
        (".5%", 0.005),
        ("1e-1%", 0.001),
        ("50% !important", 0.5),
    ]);
}

#[test]
fn a_number_still_reads_as_a_number() {
    check_values(&[
        (".5", 0.5),
        ("0", 0.0),
        ("1", 1.0),
        ("2", 1.0),
        ("-1", 0.0),
        ("-0", 0.0),
        ("+.5", 0.5),
        ("1e-1", 0.1),
        ("1E-1", 0.1),
        (".5e0", 0.5),
        ("00.5", 0.5),
        (".5 !important", 0.5),
    ]);
}

/// Rust's own float parser takes `inf`, `NaN` and `1.`, which CSS does not.
#[test]
fn what_is_not_a_css_number_or_percentage_is_dropped() {
    let cases: Vec<(String, f32, f32)> = [
        "50 %", "50%%", "%", "50%foo", "1.", ".", "1e", "0.5x", "0.5.5", "1px", "abc", "inf", "infinity", "NaN",
        "-inf",
    ]
    .into_iter()
    .map(|v| (page("", &format!("opacity:.25;opacity:{v}")), 0.25, 0.25))
    .collect();
    check(&cases, "");
}

#[test]
fn calc_of_a_number_or_of_a_percentage() {
    check_values(&[
        ("calc(50%)", 0.5),
        ("CALC(50%)", 0.5),
        ("calc(0.5)", 0.5),
        ("calc(25% + 25%)", 0.5),
        ("calc(0.25 + 0.25)", 0.5),
        ("calc(50% * 2)", 1.0),
        ("calc(50% / 2)", 0.25),
        ("calc(2 * 25%)", 0.5),
        ("calc(1 / 2)", 0.5),
        ("calc(25%*2)", 0.5),
        ("calc((25% + 25%) * 1)", 0.5),
        ("calc(calc(50%))", 0.5),
        ("calc(150%)", 1.0),
        ("calc(-1)", 0.0),
    ]);
}

#[test]
fn calc_that_mixes_or_has_a_unit_is_dropped() {
    let cases: Vec<(String, f32, f32)> = [
        "calc(0.5 + 50%)",
        "calc(0 + 50%)",
        "calc(50% + 0)",
        "calc((0 + 50%) * 1)",
        "calc(25%+25%)",
        "calc(1px)",
        "calc(1em)",
        "calc(50%) foo",
        "calc()",
        "calc(50% * 2%)",
    ]
    .into_iter()
    .map(|v| (page("", &format!("opacity:.25;opacity:{v}")), 0.25, 0.25))
    .collect();
    check(&cases, "");
}

#[test]
fn through_var_a_sheet_and_the_cascade() {
    check_values(&[("var(--p)", 0.5), ("var(--n)", 0.5), ("var(--nope, 25%)", 0.25), ("initial", 1.0), ("unset", 1.0)]);
    check(
        &[
            (page("#t{opacity:50%}", ""), 0.5, 0.5),
            (page("#t{opacity:.5;opacity:25%}", ""), 0.25, 0.25),
            (page("#t{opacity:25%;opacity:foo}", ""), 0.25, 0.25),
            (page("#t{opacity:25%;opacity:50 %}", ""), 0.25, 0.25),
        ],
        "",
    );
}

#[test]
fn a_percentage_inherits_as_its_number() {
    let html = "<!doctype html><style>body{margin:0}</style><div style=\"opacity:25%\">\
                <div id=t style=\"width:100px;height:100px;background:#f00;opacity:inherit\"></div></div>";
    check(&[(html.to_string(), 0.25, 0.0625)], "");
}

#[test]
fn a_script_write_of_a_percentage_applies() {
    let at_quarter = page("", "opacity:.25");
    check(&[(at_quarter, 0.5, 0.5)], "document.getElementById('t').style.opacity = '50%'");
}
