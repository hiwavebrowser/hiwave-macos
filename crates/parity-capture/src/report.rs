//! `--report` / `--report-text`: one page that says what is most likely
//! wrong with the captured page, facts below (dev-tools T4a).
//!
//! Built from the element diagnostics dump (`Engine::all_element_diagnostics`,
//! read-only over the final layout) and the record of the element hit at the
//! viewport's centre. Rules are deterministic and each cause names the facts
//! it read. Rules over the event log (scripts, errors, requests, load holds)
//! are not run here yet; the report says so instead of leaving them out
//! silently.

use serde::Serialize;
use serde_json::Value;

pub const REPORT_SCHEMA: &str = "hiwave-page-report/1";

/// A box at least this share of the viewport is a covering box.
const COVERING_SHARE: f64 = 0.30;
/// A covering box that takes the centre hit and at least this share is an
/// overlay (R2).
const OVERLAY_SHARE: f64 = 0.50;
/// Bands of one viewport height, from the top of the document.
const BANDS: usize = 3;
/// Selectors are page-chosen names: capped.
const MAX_SELECTOR_CHARS: usize = 64;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Report {
    pub schema: &'static str,
    pub url: Option<String>,
    pub viewport: (f64, f64),
    pub scroll: (f64, f64),
    pub document_height: f64,
    pub elements: u64,
    /// `overflow-y` and `position` of `html` and `body`.
    pub root_styles: Vec<RootStyle>,
    /// Positioned boxes covering at least 30% of the viewport, outermost
    /// only, largest first.
    pub covering: Vec<Covering>,
    /// Content per band of one viewport height, from the document's top.
    pub bands: Vec<Band>,
    /// Most likely first.
    pub ranked: Vec<Cause>,
    /// Rules this build did not run, and why.
    pub not_checked: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RootStyle {
    pub element: String,
    pub overflow_y: Option<String>,
    pub position: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Covering {
    pub selector: String,
    pub position: String,
    pub z_index: Option<String>,
    /// Share of the viewport the box covers, in percent.
    pub covers_pct: f64,
    /// The element hit at the viewport's centre is this box or inside it.
    pub takes_centre: bool,
    #[serde(skip)]
    structural: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Band {
    pub top: f64,
    pub bottom: f64,
    /// Elements holding text directly (text length > 0, no element
    /// children) whose box starts in the band.
    pub text: u64,
    /// `img`, `svg`, `picture`, `video`, `canvas` boxes starting in the band.
    pub images: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Cause {
    /// `R1`..`R9` (see the T4 design note).
    pub rule: &'static str,
    pub line: String,
}

fn style<'a>(element: &'a Value, property: &str) -> Option<&'a str> {
    element["style"][property].as_str()
}

fn rect(element: &Value) -> Option<(f64, f64, f64, f64)> {
    let b = &element["box"]["border_box"];
    Some((b["x"].as_f64()?, b["y"].as_f64()?, b["w"].as_f64()?, b["h"].as_f64()?))
}

/// `tag#id.class.class`, capped. An element with neither is named by the
/// last two steps of its structural selector (`body > div:nth-child(41)`).
fn label(element: &Value) -> String {
    let identity = &element["identity"];
    let plain = identity["id"].as_str().is_none()
        && identity["classes"].as_array().is_none_or(|classes| classes.is_empty());
    if plain {
        let steps: Vec<&str> = structural(element).split(" > ").collect();
        if steps.len() >= 2 {
            return steps[steps.len() - 2..].join(" > ");
        }
    }
    let mut out = identity["tag"].as_str().unwrap_or("?").to_string();
    if let Some(id) = identity["id"].as_str() {
        out.push('#');
        out.push_str(id);
    }
    for class in identity["classes"].as_array().into_iter().flatten().take(2) {
        if let Some(class) = class.as_str() {
            out.push('.');
            out.push_str(class);
        }
    }
    if out.chars().count() > MAX_SELECTOR_CHARS {
        out = out.chars().take(MAX_SELECTOR_CHARS - 1).collect::<String>() + "…";
    }
    out
}

fn structural(element: &Value) -> &str {
    element["identity"]["selector"].as_str().unwrap_or("")
}

fn is_within(inner: &str, outer: &str) -> bool {
    inner == outer || inner.starts_with(&format!("{outer} > "))
}

/// `html` and `body`: a page that locks scrolling often makes the body
/// `position: fixed`. That is the lock (R1), not an overlay, and what is in
/// the body is still the document's content.
fn is_root(element: &Value) -> bool {
    matches!(element["identity"]["tag"].as_str(), Some("html" | "body"))
}

fn visible(element: &Value) -> bool {
    element["box"]["has_box"].as_bool() == Some(true)
        && style(element, "visibility") != Some("hidden")
        && style(element, "opacity").and_then(|o| o.parse::<f64>().ok()) != Some(0.0)
}

/// The report for a diagnostics dump (as `all_element_diagnostics`
/// serialises it) and the record of the element at the viewport's centre.
pub fn page_report(dump: &Value, centre: Option<&Value>) -> Report {
    let page = &dump["page"];
    let viewport = (
        page["viewport"][0].as_f64().unwrap_or(0.0),
        page["viewport"][1].as_f64().unwrap_or(0.0),
    );
    let scroll = (page["scroll"][0].as_f64().unwrap_or(0.0), page["scroll"][1].as_f64().unwrap_or(0.0));
    let elements: Vec<&Value> = dump["elements"].as_array().map(|a| a.iter().collect()).unwrap_or_default();
    let centre_selector = centre.map(structural).unwrap_or("");

    let mut document_height = viewport.1;
    let mut root_styles = Vec::new();
    for element in &elements {
        let tag = element["identity"]["tag"].as_str().unwrap_or("");
        if tag == "html" || tag == "body" {
            root_styles.push(RootStyle {
                element: tag.to_string(),
                overflow_y: style(element, "overflow-y").map(str::to_string),
                position: style(element, "position").map(str::to_string),
            });
        }
        if style(element, "position") == Some("fixed") && !is_root(element) {
            continue;
        }
        if let Some((_, y, _, h)) = rect(element) {
            document_height = document_height.max(y + scroll.1 + h);
        }
    }

    let viewport_area = (viewport.0 * viewport.1).max(1.0);
    let share_of = |e: &Value| {
        let (x, y, w, h) = rect(e)?;
        let cw = (x + w).min(viewport.0) - x.max(0.0);
        let ch = (y + h).min(viewport.1) - y.max(0.0);
        Some((cw.max(0.0) * ch.max(0.0)) / viewport_area)
    };
    let positioned: Vec<(&Value, f64)> = elements
        .iter()
        .copied()
        .filter(|e| !is_root(e) && visible(e) && matches!(style(e, "position"), Some("fixed" | "absolute" | "sticky")))
        .filter_map(|e| Some((e, share_of(e)?)))
        .collect();
    // The largest fixed box on screen, whatever its share: what R1 names
    // when nothing covers the viewport (a consent bar).
    let largest_fixed = positioned
        .iter()
        .filter(|(e, share)| style(e, "position") == Some("fixed") && *share > 0.0)
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(e, share)| Covering {
            selector: label(e),
            position: "fixed".to_string(),
            z_index: style(e, "z-index").filter(|z| *z != "auto").map(str::to_string),
            covers_pct: (share * 1000.0).round() / 10.0,
            takes_centre: false,
            structural: structural(e).to_string(),
        });
    let mut covering: Vec<Covering> = Vec::new();
    let mut candidates: Vec<(&Value, f64)> =
        positioned.iter().copied().filter(|(_, share)| *share >= COVERING_SHARE).collect();
    // Tree order keeps an outer box before the boxes inside it.
    candidates.sort_by(|a, b| structural(a.0).len().cmp(&structural(b.0).len()));
    for (element, share) in candidates {
        let path = structural(element);
        if covering.iter().any(|c| is_within(path, &c.structural)) {
            continue;
        }
        covering.push(Covering {
            selector: label(element),
            position: style(element, "position").unwrap_or("").to_string(),
            z_index: style(element, "z-index").filter(|z| *z != "auto").map(str::to_string),
            covers_pct: (share * 1000.0).round() / 10.0,
            takes_centre: !centre_selector.is_empty() && is_within(centre_selector, path),
            structural: path.to_string(),
        });
    }
    covering.sort_by(|a, b| b.covers_pct.partial_cmp(&a.covers_pct).unwrap_or(std::cmp::Ordering::Equal));

    let mut bands: Vec<Band> = (0..BANDS)
        .map(|i| Band { top: i as f64 * viewport.1, bottom: (i + 1) as f64 * viewport.1, text: 0, images: 0 })
        .collect();
    // What is inside a fixed box is not the document's content.
    let fixed: Vec<&str> = elements
        .iter()
        .filter(|e| !is_root(e) && style(e, "position") == Some("fixed"))
        .map(|e| structural(e))
        .collect();
    for element in &elements {
        if !visible(element) || fixed.iter().any(|f| is_within(structural(element), f)) {
            continue;
        }
        let Some((_, y, w, h)) = rect(element) else { continue };
        if w <= 0.0 || h <= 0.0 {
            continue;
        }
        let top = y + scroll.1;
        let Some(band) = bands.iter_mut().find(|b| top >= b.top && top < b.bottom) else { continue };
        let tag = element["identity"]["tag"].as_str().unwrap_or("");
        if matches!(tag, "img" | "svg" | "picture" | "video" | "canvas") {
            band.images += 1;
        } else if element["tree"]["text_length"].as_u64().unwrap_or(0) > 0
            && element["tree"]["child_count"].as_u64() == Some(0)
        {
            band.text += 1;
        }
    }

    let mut ranked = Vec::new();
    let overlay = covering.iter().find(|c| c.takes_centre).or_else(|| covering.first()).or(largest_fixed.as_ref());
    let describe = |c: &Covering| {
        format!(
            "{} {}{} covers {}% of the viewport",
            c.position,
            c.selector,
            c.z_index.as_deref().map(|z| format!(" (z {z})")).unwrap_or_default(),
            c.covers_pct
        )
    };
    // R1: the document is taller than the viewport and cannot scroll.
    let lock = root_styles.iter().find(|r| {
        r.overflow_y.as_deref() == Some("hidden") || r.position.as_deref() == Some("fixed")
    });
    let mut named_overlay = None;
    if let (Some(lock), true) = (lock, document_height > viewport.1 + 1.0) {
        let why = if lock.overflow_y.as_deref() == Some("hidden") {
            format!("{} overflow-y:hidden", lock.element)
        } else {
            format!("{} position:fixed", lock.element)
        };
        let line = match overlay {
            Some(c) => {
                named_overlay = Some(c.selector.clone());
                format!("scroll locked ({why}) by overlay: {}", describe(c))
            }
            None => format!("scroll locked ({why}); document is {document_height:.0}px tall"),
        };
        ranked.push(Cause { rule: "R1", line });
    }
    // R2: a box over the centre of the viewport.
    for c in covering.iter().filter(|c| c.takes_centre && c.covers_pct >= OVERLAY_SHARE * 100.0) {
        if named_overlay.as_deref() != Some(c.selector.as_str()) {
            ranked.push(Cause { rule: "R2", line: format!("overlay: {}", describe(c)) });
        }
    }
    // R9: content at the top, an empty band below it inside the document.
    let top_has_content = bands.first().is_some_and(|b| b.text + b.images > 0);
    if top_has_content {
        if let Some(empty) = bands
            .iter()
            .skip(1)
            .find(|b| b.bottom <= document_height && b.text == 0 && b.images == 0)
        {
            ranked.push(Cause {
                rule: "R9",
                line: format!(
                    "below the fold is empty from y={:.0} (document {document_height:.0}px tall): no text or image starts in {:.0}..{:.0}",
                    empty.top, empty.top, empty.bottom
                ),
            });
        }
    }

    Report {
        schema: REPORT_SCHEMA,
        url: page["url"].as_str().map(str::to_string),
        viewport,
        scroll,
        document_height,
        elements: dump["total_elements"].as_u64().unwrap_or(elements.len() as u64),
        root_styles,
        covering,
        bands,
        ranked,
        not_checked: vec![
            "R3-R8 (scripts that never finished or starved, uncaught errors, blocked or failed requests, \
             late load, silent gaps): need the event-log rules (T4b)"
                .to_string(),
        ],
    }
}

/// An `inserted` or `async` script that waited at least this long between
/// its body arriving and running was starved of turns (R4).
const STARVED_MS: f64 = 3000.0;
/// `load` this long after `DOMContentLoaded` is late (R7).
const LATE_LOAD_MS: f64 = 5000.0;
/// No record for this long before the load finished is a silent gap (R8).
const SILENT_MS: f64 = 3000.0;
/// At most this many lines per rule.
const PER_RULE: usize = 3;

fn ms(v: &Value) -> Option<f64> {
    v.as_f64()
}

fn text(v: &Value) -> &str {
    v.as_str().unwrap_or("-")
}

/// The event-log rules (R3-R8) over the load's records, as the JSON Lines
/// sink writes them. Fields are read by name, so a build without T2's or
/// T3's records skips those rules and says so (the second list).
pub fn event_causes(records: &[Value]) -> (Vec<Cause>, Vec<String>) {
    let of = |kind: &'static str| records.iter().filter(move |r| r["type"] == kind);
    let mut causes = Vec::new();
    let mut not_checked = Vec::new();
    let scripts: Vec<&Value> = of("script").collect();
    let has_lifecycle = of("lifecycle").next().is_some();

    // R4: scripts starved of turns, and script requests never answered.
    if scripts.is_empty() {
        not_checked.push("R3, R4: no `script` records on this build (dev-tools T3)".to_string());
    }
    let mut starved: Vec<&&Value> = scripts
        .iter()
        .filter(|s| matches!(s["kind"].as_str(), Some("inserted" | "async")))
        .filter(|s| ms(&s["waited_ms"]).is_some_and(|w| w >= STARVED_MS))
        .collect();
    starved.sort_by(|a, b| ms(&b["waited_ms"]).partial_cmp(&ms(&a["waited_ms"])).unwrap_or(std::cmp::Ordering::Equal));
    for s in starved.iter().take(PER_RULE) {
        causes.push(Cause {
            rule: "R4",
            line: format!(
                "{} script {} arrived in {:.0} ms and waited {:.1} s for a turn",
                text(&s["kind"]),
                text(&s["script"]),
                ms(&s["fetch_ms"]).unwrap_or(0.0),
                ms(&s["waited_ms"]).unwrap_or(0.0) / 1000.0
            ),
        });
    }
    let answered: std::collections::HashSet<u64> = records
        .iter()
        .filter(|r| r["type"] == "response" || r["type"] == "failure")
        .filter_map(|r| r["id"].as_u64())
        .collect();
    let open_scripts: Vec<&Value> = of("request")
        .filter(|r| matches!(r["resource_type"].as_str(), Some("script" | "module")))
        .filter(|r| r["id"].as_u64().is_some_and(|id| !answered.contains(&id)))
        .collect();
    if let Some(first) = open_scripts.first() {
        causes.push(Cause {
            rule: "R4",
            line: format!(
                "{} script request(s) still open when the capture ended, first {} at t={:.1} s",
                open_scripts.len(),
                text(&first["url"]),
                ms(&first["t_ms"]).unwrap_or(0.0) / 1000.0
            ),
        });
    }

    // R3: scripts that never finished.
    for s in scripts
        .iter()
        .filter(|s| matches!(s["outcome"].as_str(), Some("skipped" | "over_budget" | "stopped" | "fetch_failed")))
        .filter(|s| s["reason"] != "nomodule")
        .take(PER_RULE)
    {
        causes.push(Cause {
            rule: "R3",
            line: format!(
                "{} script {} {} ({})",
                text(&s["kind"]),
                text(&s["script"]),
                text(&s["outcome"]),
                text(&s["reason"])
            ),
        });
    }

    // R6: requests the shield blocked, then other failures; scripts and
    // stylesheets first.
    let urls: std::collections::HashMap<u64, (&str, &str)> = of("request")
        .filter_map(|r| Some((r["id"].as_u64()?, (text(&r["url"]), text(&r["resource_type"])))))
        .collect();
    let mut failures: Vec<&Value> = of("failure").collect();
    let weight = |f: &Value| {
        let blocked = !f["blocked_by"].is_null();
        let kind = f["id"].as_u64().and_then(|id| urls.get(&id)).map(|u| u.1).unwrap_or("");
        (!blocked, !matches!(kind, "script" | "module" | "stylesheet"))
    };
    failures.sort_by_key(|f| weight(f));
    if !records.iter().any(|r| r.get("blocked_by").is_some()) {
        not_checked.push("R6 shield list/rule: no `blocked_by` field on this build (dev-tools T2)".to_string());
    }
    for f in failures.iter().filter(|f| f["stage"] != "cancelled").take(PER_RULE) {
        let (url, kind) = f["id"].as_u64().and_then(|id| urls.get(&id)).copied().unwrap_or(("-", "-"));
        let line = match f["blocked_by"].as_object() {
            Some(b) => format!(
                "shield blocked {kind} {url} ({} {})",
                b.get("list").and_then(Value::as_str).unwrap_or("-"),
                b.get("rule").and_then(Value::as_str).unwrap_or("-")
            ),
            None => format!("{kind} {url} failed: {} ({})", text(&f["stage"]), text(&f["reason"])),
        };
        causes.push(Cause { rule: "R6", line });
    }

    // R5: uncaught errors, grouped by name and place.
    let mut errors: Vec<(String, usize)> = Vec::new();
    for e in of("script_error") {
        let at = &e["location"];
        let key = format!(
            "uncaught {}{} in {}",
            if e["name"].as_str().unwrap_or("").is_empty() { text(&e["value"]) } else { text(&e["name"]) },
            if e["unhandled_rejection"] == true { " (unhandled rejection)" } else { "" },
            if at.is_null() {
                text(&e["script"]).to_string()
            } else {
                format!("{}:{}:{}", text(&at["url"]), at["line"], at["column"])
            }
        );
        match errors.iter_mut().find(|(k, _)| *k == key) {
            Some((_, n)) => *n += 1,
            None => errors.push((key, 1)),
        }
    }
    for (key, n) in errors.into_iter().take(PER_RULE) {
        let line = if n > 1 { format!("{key} (x{n})") } else { key };
        causes.push(Cause { rule: "R5", line });
    }

    // R7: load long after DOMContentLoaded, and what it waited for.
    if !has_lifecycle {
        not_checked.push("R7: no `lifecycle` records on this build (dev-tools T3)".to_string());
    }
    let at = |event: &str| of("lifecycle").find(|r| r["event"] == event).and_then(|r| ms(&r["t_ms"]));
    if let (Some(dcl), Some(load)) = (at("DOMContentLoaded"), at("load")) {
        if load - dcl >= LATE_LOAD_MS {
            let held = of("lifecycle")
                .find(|r| r["event"] == "load")
                .map(|r| {
                    r["held_by"]
                        .as_array()
                        .map(|a| a.iter().take(PER_RULE).map(|s| text(s).to_string()).collect::<Vec<_>>().join(", "))
                        .unwrap_or_default()
                })
                .unwrap_or_default();
            causes.push(Cause {
                rule: "R7",
                line: format!(
                    "load fired {:.1} s after DOMContentLoaded; it waited for: {}",
                    (load - dcl) / 1000.0,
                    if held.is_empty() { "no script (see R8)".to_string() } else { held }
                ),
            });
        }
    }

    // R8: the log silent for a while before the load finished.
    let finished = of("load_finished").next().and_then(|r| ms(&r["t_ms"]));
    let times: Vec<f64> = records.iter().filter_map(|r| ms(&r["t_ms"])).collect();
    let mut gaps: Vec<(f64, f64)> = times
        .windows(2)
        .filter(|w| finished.is_none_or(|end| w[1] <= end))
        .map(|w| (w[0], w[1] - w[0]))
        .filter(|(_, gap)| *gap >= SILENT_MS)
        .collect();
    gaps.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    for (from, gap) in gaps.into_iter().take(PER_RULE) {
        causes.push(Cause {
            rule: "R8",
            line: format!(
                "event log silent {:.1} s from t={:.1} s before the load finished: no script ended, no request moved",
                gap / 1000.0,
                from / 1000.0
            ),
        });
    }
    (causes, not_checked)
}

/// Add the event-log rules to a layout report: layout causes keep the top
/// (R1, R2), then an empty band (R9), then the event causes in the design
/// note's order.
pub fn apply_events(report: &mut Report, records: &[Value]) {
    let (events, not_checked) = event_causes(records);
    let order = ["R1", "R2", "R9", "R4", "R3", "R6", "R5", "R7", "R8"];
    report.ranked.extend(events);
    report.ranked.sort_by_key(|c| order.iter().position(|r| *r == c.rule).unwrap_or(order.len()));
    report.not_checked.retain(|n| !n.starts_with("R3-R8"));
    report.not_checked.extend(not_checked);
}

/// The report as one page of text.
pub fn report_text(report: &Report) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "diagnose {}  viewport {:.0}x{:.0}  document {:.0}px tall  {} elements\n\n",
        report.url.as_deref().unwrap_or("-"),
        report.viewport.0,
        report.viewport.1,
        report.document_height,
        report.elements
    ));
    out.push_str("Most likely:\n");
    if report.ranked.is_empty() {
        out.push_str("  (no rule fired)\n");
    }
    for (i, cause) in report.ranked.iter().enumerate() {
        out.push_str(&format!("  {}. [{}] {}\n", i + 1, cause.rule, cause.line));
    }
    out.push_str("\nFacts:\n");
    for r in &report.root_styles {
        out.push_str(&format!(
            "  {}: overflow-y {}, position {}\n",
            r.element,
            r.overflow_y.as_deref().unwrap_or("-"),
            r.position.as_deref().unwrap_or("-")
        ));
    }
    if report.covering.is_empty() {
        out.push_str("  covering boxes: none\n");
    }
    for c in &report.covering {
        out.push_str(&format!(
            "  covering: {} {} z {} covers {}%{}\n",
            c.position,
            c.selector,
            c.z_index.as_deref().unwrap_or("auto"),
            c.covers_pct,
            if c.takes_centre { ", takes the centre" } else { "" }
        ));
    }
    for b in &report.bands {
        out.push_str(&format!("  y {:.0}..{:.0}: text {}, images {}\n", b.top, b.bottom, b.text, b.images));
    }
    out.push_str("\nNot checked:\n");
    for n in &report.not_checked {
        out.push_str(&format!("  {n}\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn lines(records: &[Value]) -> Vec<String> {
        event_causes(records).0.iter().map(|c| format!("{} {}", c.rule, c.line)).collect()
    }

    /// yahoo, 2026-10-24 (T3 binary): the ad loader arrived in 196 ms and
    /// waited 24.5 s for a turn; a page chunk's request never answered.
    #[test]
    fn a_starved_inserted_script_and_an_open_script_request_are_r4() {
        let records = vec![
            json!({"seq": 1, "t_ms": 5000.0, "type": "request", "id": 7, "url": "https://x.test/chunk.js", "resource_type": "script"}),
            json!({"seq": 2, "t_ms": 5766.0, "type": "lifecycle", "event": "DOMContentLoaded", "held_by": [], "held_by_count": 0}),
            json!({"seq": 3, "t_ms": 5766.0, "type": "lifecycle", "event": "load", "held_by": [], "held_by_count": 0}),
            json!({"seq": 4, "t_ms": 5881.0, "type": "load_finished"}),
            json!({"seq": 5, "t_ms": 32229.0, "type": "script", "script": "https://s.test/benji.js", "kind": "inserted",
                   "outcome": "ran", "reason": null, "fetch_ms": 196.0, "waited_ms": 24519.0, "held": null}),
        ];
        assert_eq!(
            lines(&records),
            vec![
                "R4 inserted script https://s.test/benji.js arrived in 196 ms and waited 24.5 s for a turn",
                "R4 1 script request(s) still open when the capture ended, first https://x.test/chunk.js at t=5.0 s",
            ]
        );
    }

    #[test]
    fn scripts_that_never_finished_are_r3_but_nomodule_is_not() {
        let script = |name: &str, outcome: &str, reason: &str| {
            json!({"t_ms": 1.0, "type": "script", "script": name, "kind": "parser", "outcome": outcome, "reason": reason})
        };
        let records = vec![
            script("inline#1", "skipped", "nomodule"),
            script("https://x.test/big.js", "skipped", "too large to run inside the script budget"),
            script("inline#3", "stopped", "script budget"),
        ];
        assert_eq!(
            lines(&records),
            vec![
                "R3 parser script https://x.test/big.js skipped (too large to run inside the script budget)",
                "R3 parser script inline#3 stopped (script budget)",
            ]
        );
    }

    #[test]
    fn a_shield_block_comes_before_other_failures_and_cancels_are_left_out() {
        let records = vec![
            json!({"t_ms": 1.0, "type": "request", "id": 1, "url": "https://x.test/a.png", "resource_type": "image"}),
            json!({"t_ms": 1.0, "type": "request", "id": 2, "url": "https://ads.test/gpt.js", "resource_type": "script"}),
            json!({"t_ms": 1.0, "type": "request", "id": 3, "url": "https://x.test/b.js", "resource_type": "script"}),
            json!({"t_ms": 2.0, "type": "failure", "id": 1, "stage": "timeout", "reason": "timed out", "blocked_by": null}),
            json!({"t_ms": 2.0, "type": "failure", "id": 2, "stage": "policy", "reason": "blocked",
                   "blocked_by": {"list": "EasyList", "rule": "||ads.test^"}}),
            json!({"t_ms": 2.0, "type": "failure", "id": 3, "stage": "cancelled", "reason": "abandoned", "blocked_by": null}),
        ];
        assert_eq!(
            lines(&records),
            vec![
                "R6 shield blocked script https://ads.test/gpt.js (EasyList ||ads.test^)",
                "R6 image https://x.test/a.png failed: timeout (timed out)",
            ]
        );
    }

    /// cnn, 2026-10-24 (T3 binary): load 8.6 s after DOMContentLoaded, and
    /// the log silent from 11.85 s to 17.76 s.
    #[test]
    fn a_late_load_is_r7_and_a_silent_stretch_before_it_is_r8() {
        let records = vec![
            json!({"t_ms": 9751.0, "type": "lifecycle", "event": "DOMContentLoaded", "held_by": [], "held_by_count": 0}),
            json!({"t_ms": 11846.0, "type": "response", "id": 9}),
            json!({"t_ms": 17758.0, "type": "request", "id": 10, "url": "https://x.test/f", "resource_type": "fetch"}),
            json!({"t_ms": 18397.0, "type": "lifecycle", "event": "load",
                   "held_by": ["https://x.test/otSDKStub.js", "https://x.test/otBannerSdk.js"], "held_by_count": 2}),
            json!({"t_ms": 19151.0, "type": "load_finished"}),
            json!({"t_ms": 30000.0, "type": "relayout"}),
        ];
        assert_eq!(
            lines(&records),
            vec![
                "R7 load fired 8.6 s after DOMContentLoaded; it waited for: https://x.test/otSDKStub.js, https://x.test/otBannerSdk.js",
                "R8 event log silent 5.9 s from t=11.8 s before the load finished: no script ended, no request moved",
            ]
        );
    }

    /// T1's `script_error`: grouped by name and place, a rejection marked.
    #[test]
    fn uncaught_errors_are_r5_grouped_by_place() {
        let error = |line: u32, rejection: bool| {
            json!({"t_ms": 1.0, "type": "script_error", "name": "TypeError", "message": null,
                   "location": {"url": "https://x.test/7730.js", "line": line, "column": 12},
                   "unhandled_rejection": rejection, "script": "https://x.test/7730.js", "value": "error"})
        };
        assert_eq!(
            lines(&[error(3, false), error(3, false), error(9, true)]),
            vec![
                "R5 uncaught TypeError in https://x.test/7730.js:3:12 (x2)",
                "R5 uncaught TypeError (unhandled rejection) in https://x.test/7730.js:9:12",
            ]
        );
    }

    #[test]
    fn a_build_without_t2_or_t3_records_says_which_rules_it_skipped() {
        let (causes, skipped) = event_causes(&[json!({"t_ms": 1.0, "type": "load_finished"})]);
        assert!(causes.is_empty());
        assert_eq!(skipped.len(), 3, "{skipped:?}");
        assert!(skipped[0].starts_with("R3, R4") && skipped[1].starts_with("R6") && skipped[2].starts_with("R7"));
    }
}
