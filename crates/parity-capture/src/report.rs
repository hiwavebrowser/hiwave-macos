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
    /// `R1`..`R10` (see the T4 design note).
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
    } else if bands.first().is_some() {
        // R10: nothing to read at all. yahoo with #777 merged ended as 77
        // elements and no text after React's error boundary took the page.
        ranked.push(Cause {
            rule: "R10",
            line: format!(
                "the first viewport has no text and no image ({} elements, document {document_height:.0}px tall)",
                dump["total_elements"].as_u64().unwrap_or(elements.len() as u64)
            ),
        });
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

/// R5 lines about the page's own site start with this, and rank before R4.
const OWN_SITE: &str = "the page's own script: ";

/// The last two host labels (`www.yahoo.com` -> `yahoo.com`). Close enough
/// for ranking; a two-label public suffix (`co.uk`) reads as one site. An
/// IPv4 address is its own site, whole (an IPv6 one is not split out by
/// `host_of`). No host (`file://`) is no site, so a `file://` page ranks no
/// script error as its own.
fn site_of(url: &str) -> Option<String> {
    let host = host_of(url).filter(|h| !h.is_empty())?;
    if host.parse::<std::net::Ipv4Addr>().is_ok() {
        return Some(host);
    }
    let labels: Vec<&str> = host.split('.').collect();
    Some(labels[labels.len().saturating_sub(2)..].join("."))
}

fn host_of(url: &str) -> Option<String> {
    Some(url.split("://").nth(1)?.split(['/', ':', '?', '#']).next()?.to_string())
}

/// The site of the URL in an R5 key (`uncaught X in <url>:line:col`,
/// `logged X thrown in <url>:line:col`).
fn site_of_error(key: &str) -> Option<String> {
    site_of(key.split(" in ").nth(1)?)
}

/// The error name in a console message: the shape `<TypeError message len=46>`
/// or the kept text `TypeError: ...`.
fn error_name(message: &str) -> &str {
    let name = message.trim_start_matches('<');
    let end = name.find([' ', ':', '>']).unwrap_or(name.len());
    if end == 0 { "error" } else { &name[..end] }
}

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
    // A record is written when its script is over, so it ran over
    // [t - ran, t] and waited over [t - ran - waited, t - ran]. A wait spent
    // while other scripts ran is a queue, not starvation: only the idle part
    // of the wait counts.
    let run_span = |s: &Value| {
        let end = ms(&s["t_ms"]).unwrap_or(0.0);
        (end - ms(&s["ran_ms"]).unwrap_or(0.0), end)
    };
    let busy_during = |me: &Value, from: f64, to: f64| -> f64 {
        scripts
            .iter()
            .filter(|o| !std::ptr::eq(**o, me))
            .map(|o| {
                let (a, b) = run_span(o);
                (b.min(to) - a.max(from)).max(0.0)
            })
            .sum::<f64>()
            .min(to - from)
    };
    // Before the first page script runs, every script waits on the load
    // itself, not for a turn (yahoo: bodies by 1.7 s, first script at 5.8 s,
    // after first layout and the last image). That is one fact, said once.
    let first_start = scripts.iter().map(|s| run_span(s).0).fold(f64::INFINITY, f64::min);
    let early: Vec<f64> = scripts
        .iter()
        .filter_map(|s| Some(run_span(s).0 - ms(&s["waited_ms"])?))
        .filter(|arrived| first_start - arrived >= STARVED_MS)
        .collect();
    if let Some(first_arrival) = early.iter().copied().reduce(f64::min) {
        let kind_of: std::collections::HashMap<u64, &str> = of("request")
            .filter_map(|r| Some((r["id"].as_u64()?, r["resource_type"].as_str()?)))
            .collect();
        let last_answer = records
            .iter()
            .filter(|r| r["type"] == "response" || r["type"] == "failure")
            .filter(|r| ms(&r["t_ms"]).is_some_and(|t| t <= first_start))
            .last();
        let mut line = format!(
            "no page script ran until t={:.1} s: {} script body(ies) had arrived {:.1} s or more before (first at t={:.1} s)",
            first_start / 1000.0,
            early.len(),
            STARVED_MS / 1000.0,
            first_arrival / 1000.0
        );
        if let Some(t) = of("first_layout").next().and_then(|r| ms(&r["t_ms"])) {
            line.push_str(&format!("; first layout at t={:.1} s", t / 1000.0));
        }
        if let Some(r) = last_answer {
            let kind = r["id"].as_u64().and_then(|id| kind_of.get(&id)).copied().unwrap_or("request");
            line.push_str(&format!("; last answer before it: {kind} at t={:.1} s", ms(&r["t_ms"]).unwrap_or(0.0) / 1000.0));
        }
        causes.push(Cause { rule: "R4", line });
    }
    let mut starved: Vec<(&Value, f64, f64)> = scripts
        .iter()
        .filter(|s| matches!(s["kind"].as_str(), Some("inserted" | "async")))
        .filter_map(|s| {
            let (start, _) = run_span(s);
            // Only the wait after the first script ran can be a missed turn.
            let from = (start - ms(&s["waited_ms"])?).max(first_start);
            let waited = (start - from).max(0.0);
            let busy = busy_during(s, from, start);
            (waited - busy >= STARVED_MS).then_some((*s, waited, busy))
        })
        .collect();
    starved.sort_by(|a, b| (b.1 - b.2).partial_cmp(&(a.1 - a.2)).unwrap_or(std::cmp::Ordering::Equal));
    for (s, waited, busy) in starved.iter().take(PER_RULE) {
        let queued = if *busy >= 100.0 {
            format!(" ({:.1} s of it behind other scripts running)", busy / 1000.0)
        } else {
            String::new()
        };
        causes.push(Cause {
            rule: "R4",
            line: format!(
                "{} script {} arrived in {:.0} ms and waited {:.1} s for a turn{queued}",
                text(&s["kind"]),
                text(&s["script"]),
                ms(&s["fetch_ms"]).unwrap_or(0.0),
                waited / 1000.0
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
    let mut errors: Vec<(String, usize, f64)> = Vec::new();
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
        match errors.iter_mut().find(|(k, _, _)| *k == key) {
            Some((_, n, _)) => *n += 1,
            None => errors.push((key, 1, ms(&e["t_ms"]).unwrap_or(0.0))),
        }
    }
    // An error the page caught and passed to console.error carries its own
    // stack (`error_stack`); it is named by where it was thrown. yahoo with
    // #777 merged: two such TypeErrors, no uncaught one, and a blank page.
    for c in of("console").filter(|c| c["level"] == "error") {
        let Some(thrown) = c["error_stack"].as_str().and_then(|s| s.lines().next()) else {
            continue;
        };
        let place = thrown.trim().trim_start_matches("at ");
        let place = place.rsplit_once(" (").map_or(place, |(_, p)| p.trim_end_matches(')'));
        let key = format!("logged {} thrown in {place}", error_name(text(&c["message"])));
        match errors.iter_mut().find(|(k, _, _)| *k == key) {
            Some((_, n, _)) => *n += 1,
            None => errors.push((key, 1, ms(&c["t_ms"]).unwrap_or(0.0))),
        }
    }
    // An error in the page's own site's script ranks first of the event
    // causes (design note R5): yahoo's chunk 7730 SyntaxError is the app
    // bundle failing, where an ad script's error is noise. Its line says how
    // many fetch/XHR went to the page's own host afterwards (yahoo: none to
    // www.yahoo.com, so the feed never asked for its data). The host, not the
    // site: yahoo's ad and analytics hosts share the site.
    let page_url = of("navigation_start").next().and_then(|r| r["url"].as_str());
    let site = page_url.and_then(site_of);
    let page_host = page_url.and_then(host_of);
    let own_calls_after = |t: f64| {
        of("request")
            .filter(|r| matches!(r["resource_type"].as_str(), Some("fetch" | "xhr")))
            .filter(|r| ms(&r["t_ms"]).is_some_and(|at| at >= t))
            .filter(|r| r["url"].as_str().and_then(host_of) == page_host)
            .count()
    };
    errors.sort_by_key(|(key, _, _)| !(site.is_some() && site_of_error(key) == site));
    for (key, n, first) in errors.into_iter().take(PER_RULE) {
        let mut line = if n > 1 { format!("{key} (x{n})") } else { key };
        if site.is_some() && site_of_error(&line) == site {
            let calls = own_calls_after(first);
            let host = page_host.as_deref().unwrap_or("-");
            line = format!("{OWN_SITE}{line}; then {calls} fetch/XHR to {host}");
        }
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

    // R8: the log silent for a while before the load finished, or while a
    // script body was in and waiting to run (yahoo: nine inserted chunks in
    // at 11.3 s, nothing recorded until 15.4 s, well after load).
    let finished = of("load_finished").next().and_then(|r| ms(&r["t_ms"]));
    // A script running is not silence: its start is a point too, and its run
    // span is taken out of any gap it falls in.
    let spans: Vec<(f64, f64)> = scripts.iter().map(|s| run_span(s)).collect();
    let waits: Vec<(f64, f64)> = scripts
        .iter()
        .filter_map(|s| {
            let start = run_span(s).0;
            Some((start - ms(&s["waited_ms"])?, start))
        })
        .collect();
    let mut times: Vec<f64> = records.iter().filter_map(|r| ms(&r["t_ms"])).chain(spans.iter().map(|s| s.0)).collect();
    times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mut gaps: Vec<(f64, f64, usize)> = times
        .windows(2)
        .filter_map(|w| {
            let waiting = waits.iter().filter(|(a, b)| *a <= w[0] && *b >= w[1]).count();
            let pending = finished.is_none_or(|end| w[1] <= end);
            if !pending && waiting == 0 {
                return None;
            }
            let ran: f64 = spans.iter().map(|(a, b)| (b.min(w[1]) - a.max(w[0])).max(0.0)).sum();
            Some((w[0], (w[1] - w[0] - ran).max(0.0), if pending { 0 } else { waiting }))
        })
        .filter(|(_, gap, _)| *gap >= SILENT_MS)
        .collect();
    gaps.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    for (from, gap, waiting) in gaps.into_iter().take(PER_RULE) {
        let when = if waiting == 0 {
            "before the load finished".to_string()
        } else {
            format!("with {waiting} script(s) in and waiting to run")
        };
        causes.push(Cause {
            rule: "R8",
            line: format!(
                "event log silent {:.1} s from t={:.1} s {when}: no script ran, no request moved",
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
    // "R5*" is an R5 line about the page's own site. "R5!" is an uncaught
    // one when the first viewport is blank (R10): the cause goes above the
    // symptom (Atlas 10-26; yahoo's chunk 7730 SyntaxError under R10).
    let order = ["R5!", "R1", "R2", "R10", "R9", "R5*", "R4", "R3", "R6", "R5", "R7", "R8"];
    let blank = report.ranked.iter().any(|c| c.rule == "R10");
    let uncaught = format!("{OWN_SITE}uncaught ");
    report.ranked.extend(events);
    report.ranked.sort_by_key(|c| {
        let rule = match c.rule {
            "R5" if blank && c.line.starts_with(&uncaught) => "R5!",
            "R5" if c.line.starts_with(OWN_SITE) => "R5*",
            rule => rule,
        };
        order.iter().position(|r| *r == rule).unwrap_or(order.len())
    });
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
            json!({"seq": 5, "t_ms": 5838.0, "type": "script", "script": "inline#34", "kind": "parser",
                   "outcome": "ran", "ran_ms": 1.0}),
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

    /// yahoo, 2026-10-24 (T3 + T4b): async scripts that waited behind parser
    /// scripts read as starved. A wait covered by other scripts running is a
    /// queue; only the idle part counts.
    #[test]
    fn an_async_script_queued_behind_running_scripts_is_not_starved() {
        let script = |name: &str, kind: &str, end: f64, waited: f64, ran: f64| {
            json!({"t_ms": end, "type": "script", "script": name, "kind": kind, "outcome": "ran",
                   "fetch_ms": 50.0, "waited_ms": waited, "ran_ms": ran})
        };
        let records = vec![
            // Parser scripts run 1.0..4.9 s and 9.0..11.0 s.
            script("https://x.test/p1.js", "parser", 4900.0, 0.0, 3900.0),
            script("https://x.test/p2.js", "parser", 11000.0, 0.0, 2000.0),
            // Waited 1.0..4.9 s, all of it behind p1: a queue.
            script("https://x.test/queued.js", "async", 5000.0, 3900.0, 100.0),
            // Waited 3.0..11.0 s: 4.0 s behind p1, queued.js and p2
            // (1.9 + 0.1 + 2.0), 4.0 s idle.
            script("https://x.test/idle.js", "async", 11100.0, 8000.0, 100.0),
        ];
        assert_eq!(
            lines(&records),
            vec![
                "R4 async script https://x.test/idle.js arrived in 50 ms and waited 8.0 s for a turn (4.0 s of it behind other scripts running)",
                // The idle part of that wait, once; p1's and p2's runs are not silence.
                "R8 event log silent 4.0 s from t=5.0 s before the load finished: no script ran, no request moved",
            ]
        );
    }

    /// yahoo, 2026-10-24 (LW + T3 + T4 stack): the async chunks' bodies were
    /// in by 1.7 s, but no page script ran until 5.8 s, after first layout
    /// and the last image. That is one line about the load, not one
    /// "starved" line per chunk.
    #[test]
    fn scripts_held_until_the_load_reaches_its_script_phase_are_one_r4_line() {
        let records = vec![
            json!({"t_ms": 900.0, "type": "request", "id": 8, "resource_type": "script", "url": "https://x.test/a.js"}),
            json!({"t_ms": 900.0, "type": "request", "id": 9, "resource_type": "image", "url": "https://x.test/i.png"}),
            json!({"t_ms": 1700.0, "type": "response", "id": 8}),
            json!({"t_ms": 4532.0, "type": "first_layout"}),
            json!({"t_ms": 5816.0, "type": "response", "id": 9}),
            json!({"t_ms": 5838.0, "type": "script", "script": "inline#34", "kind": "parser", "outcome": "ran", "ran_ms": 2.0}),
            json!({"t_ms": 6023.0, "type": "script", "script": "https://x.test/a.js", "kind": "async", "outcome": "ran",
                   "fetch_ms": 800.0, "waited_ms": 4153.0, "ran_ms": 170.0}),
        ];
        assert_eq!(
            lines(&records),
            vec![
                "R4 no page script ran until t=5.8 s: 1 script body(ies) had arrived 3.0 s or more before (first at t=1.7 s); first layout at t=4.5 s; last answer before it: image at t=5.8 s",
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
                "R8 event log silent 5.9 s from t=11.8 s before the load finished: no script ran, no request moved",
            ]
        );
    }

    /// yahoo, 2026-10-24 (LW + T3 + T4 stack): long after load, inserted
    /// chunks were in at 11.3 s and nothing was recorded until 15.4 s.
    #[test]
    fn a_silent_stretch_after_load_with_a_script_waiting_is_r8() {
        let records = vec![
            json!({"t_ms": 5000.0, "type": "script", "script": "inline#1", "kind": "parser", "outcome": "ran", "ran_ms": 10.0}),
            json!({"t_ms": 6602.0, "type": "load_finished"}),
            json!({"t_ms": 11318.0, "type": "response", "id": 145}),
            json!({"t_ms": 15364.0, "type": "request", "id": 152, "resource_type": "fetch", "url": "https://x.test/cfg"}),
            json!({"t_ms": 16745.0, "type": "script", "script": "https://x.test/67222.js", "kind": "inserted", "outcome": "ran",
                   "fetch_ms": 1853.0, "waited_ms": 5444.0, "ran_ms": 11.0}),
        ];
        assert_eq!(
            lines(&records),
            vec![
                "R4 inserted script https://x.test/67222.js arrived in 1853 ms and waited 5.4 s for a turn",
                "R8 event log silent 4.0 s from t=11.3 s with 1 script(s) in and waiting to run: no script ran, no request moved",
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

    /// #802 review nit: a `file://` page has no host, so it has no site and
    /// no script error is "its own"; an IPv4 host is a site whole, not its
    /// last two octets.
    #[test]
    fn a_file_page_has_no_site_and_an_ip_host_is_whole() {
        assert_eq!(site_of("file:///tmp/fixture/index.html"), None);
        assert_eq!(site_of("http://192.168.1.20:8080/a.js"), Some("192.168.1.20".to_string()));
        assert_ne!(site_of("http://10.0.1.20/"), site_of("http://10.9.1.20/"));
        assert_eq!(site_of("https://www.yahoo.com/x.js"), Some("yahoo.com".to_string()));
    }

    /// yahoo, 2026-10-24: the site's own chunk 7730 throws a SyntaxError
    /// and the site makes no fetch of its own; that is ranked above the
    /// script waits (design note R5), an ad script's error is not.
    #[test]
    fn an_error_in_the_pages_own_site_ranks_before_r4() {
        let records = vec![
            json!({"t_ms": 0.0, "type": "navigation_start", "url": "https://www.x.test/"}),
            json!({"t_ms": 900.0, "type": "script_error", "name": "TypeError", "message": null,
                   "location": {"url": "https://ads.y.test/ad.js", "line": 1, "column": 5},
                   "unhandled_rejection": false, "script": "https://ads.y.test/ad.js", "value": "error"}),
            json!({"t_ms": 1000.0, "type": "script_error", "name": "SyntaxError", "message": null,
                   "location": {"url": "https://www.x.test/chunks/7730.js", "line": 1, "column": 6278},
                   "unhandled_rejection": false, "script": "https://www.x.test/chunks/7730.js", "value": "error"}),
            // Counted: only the www.x.test call after the error. Not: one
            // before it, another site's, and another host of the same site.
            json!({"t_ms": 800.0, "type": "request", "id": 3, "resource_type": "fetch", "url": "https://www.x.test/api/early"}),
            json!({"t_ms": 2000.0, "type": "request", "id": 4, "resource_type": "fetch", "url": "https://ads.y.test/bid"}),
            json!({"t_ms": 2100.0, "type": "request", "id": 5, "resource_type": "xhr", "url": "https://pbd.x.test/analytics"}),
            json!({"t_ms": 2200.0, "type": "request", "id": 6, "resource_type": "fetch", "url": "https://www.x.test/api/feed"}),
            json!({"t_ms": 5000.0, "type": "script", "script": "inline#1", "kind": "parser", "outcome": "ran", "ran_ms": 10.0}),
            json!({"t_ms": 16745.0, "type": "script", "script": "https://x.test/67222.js", "kind": "inserted", "outcome": "ran",
                   "fetch_ms": 1853.0, "waited_ms": 5444.0, "ran_ms": 11.0}),
            json!({"t_ms": 16800.0, "type": "load_finished"}),
        ];
        let mut report = page_report(&json!({"page": {"url": "https://www.x.test/", "viewport": [1280.0, 800.0]}}), None);
        apply_events(&mut report, &records);
        let ranked: Vec<String> = report.ranked.iter().map(|c| format!("{} {}", c.rule, c.line)).collect();
        // The dump has no elements, so R10 fires; the own-site uncaught error
        // is its cause and goes above it. R8 lines follow (the records are
        // sparse).
        assert!(ranked[1].starts_with("R10 "), "{ranked:?}");
        assert_eq!(
            [ranked[0].as_str(), ranked[2].as_str(), ranked[3].as_str()],
            [
                "R5 the page's own script: uncaught SyntaxError in https://www.x.test/chunks/7730.js:1:6278; then 1 fetch/XHR to www.x.test",
                "R4 inserted script https://x.test/67222.js arrived in 1853 ms and waited 5.4 s for a turn",
                "R5 uncaught TypeError in https://ads.y.test/ad.js:1:5",
            ]
        );
    }

    /// Atlas 10-26: an uncaught own-site error ranks above R10 (cause over
    /// symptom); an ad script's error, an own-site error the page caught and
    /// logged, and an own-site error on a page that painted do not.
    #[test]
    fn an_uncaught_own_site_error_ranks_above_a_blank_first_viewport() {
        let nav = json!({"t_ms": 0.0, "type": "navigation_start", "url": "https://www.x.test/"});
        let uncaught = |url: &str| {
            json!({"t_ms": 1000.0, "type": "script_error", "name": "SyntaxError", "message": null,
                   "location": {"url": url, "line": 1, "column": 6278},
                   "unhandled_rejection": false, "script": url, "value": "error"})
        };
        let logged = json!({"t_ms": 1200.0, "type": "console", "level": "error",
                            "message": "<TypeError message len=46>",
                            "error_stack": "    at f (https://www.x.test/app.js:3:9)"});
        let ranked = |records: &[Value], painted: bool| {
            let mut report = page_report(&json!({"page": {"url": "https://www.x.test/", "viewport": [1280.0, 800.0]}}), None);
            if painted {
                report.ranked.retain(|c| c.rule != "R10");
            }
            apply_events(&mut report, records);
            report.ranked.iter().map(|c| format!("{} {}", c.rule, c.line)).collect::<Vec<_>>()
        };
        let own = uncaught("https://www.x.test/chunks/7730.js");
        let own_line = "R5 the page's own script: uncaught SyntaxError in https://www.x.test/chunks/7730.js:1:6278; then 0 fetch/XHR to www.x.test";
        let blank = ranked(&[nav.clone(), own.clone()], false);
        assert_eq!(blank[0], own_line, "{blank:?}");
        assert!(blank[1].starts_with("R10 "), "{blank:?}");
        let ad = ranked(&[nav.clone(), uncaught("https://ads.y.test/ad.js")], false);
        assert!(ad[0].starts_with("R10 "), "{ad:?}");
        let caught = ranked(&[nav.clone(), logged], false);
        assert!(caught[0].starts_with("R10 "), "{caught:?}");
        // No R10: the own-site error keeps its R5* place.
        let painted = ranked(&[nav, own], true);
        assert!(painted.iter().all(|l| !l.starts_with("R10 ")), "{painted:?}");
        assert!(painted.contains(&own_line.to_string()), "{painted:?}");
    }

    /// yahoo with #777 merged: no uncaught error, but two TypeErrors caught
    /// and passed to console.error, then a blank page. Named by where they
    /// were thrown (the first `error_stack` frame), own site first.
    #[test]
    fn an_error_caught_and_logged_with_its_stack_is_r5() {
        let logged = |t: f64, stack: &str| {
            json!({"t_ms": t, "type": "console", "level": "error", "message": "<TypeError message len=46>",
                   "location": {"url": "https://www.x.test/7730.js", "line": 52, "column": 1849},
                   "stack": "    at https://www.x.test/7730.js:52:1854", "error_stack": stack})
        };
        let records = vec![
            json!({"t_ms": 0.0, "type": "navigation_start", "url": "https://www.x.test/"}),
            json!({"t_ms": 1.0, "type": "console", "level": "error", "message": "<string len=5>",
                   "location": {"url": "https://ads.y.test/a.js", "line": 1, "column": 1}}),
            logged(7808.0, "    at https://www.x.test/7730.js:1:6198\n    at https://www.x.test/7730.js:1:6074"),
            logged(9812.0, "    at d (https://www.x.test/7730.js:9:40)\n    at oQ (https://www.x.test/87c7.js:1:71835)"),
            logged(9900.0, "    at d (https://www.x.test/7730.js:9:40)"),
        ];
        assert_eq!(
            lines(&records)[..2],
            [
                "R5 the page's own script: logged TypeError thrown in https://www.x.test/7730.js:1:6198; then 0 fetch/XHR to www.x.test",
                "R5 the page's own script: logged TypeError thrown in https://www.x.test/7730.js:9:40 (x2); then 0 fetch/XHR to www.x.test",
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
