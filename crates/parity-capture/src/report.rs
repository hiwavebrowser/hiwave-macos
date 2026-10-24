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

/// `tag#id.class.class`, capped.
fn label(element: &Value) -> String {
    let identity = &element["identity"];
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
        if style(element, "position") == Some("fixed") {
            continue;
        }
        if let Some((_, y, _, h)) = rect(element) {
            document_height = document_height.max(y + scroll.1 + h);
        }
    }

    let viewport_area = (viewport.0 * viewport.1).max(1.0);
    let mut covering: Vec<Covering> = Vec::new();
    let mut candidates: Vec<(&Value, f64)> = elements
        .iter()
        .copied()
        .filter(|e| visible(e) && matches!(style(e, "position"), Some("fixed" | "absolute" | "sticky")))
        .filter_map(|e| {
            let (x, y, w, h) = rect(e)?;
            let cw = (x + w).min(viewport.0) - x.max(0.0);
            let ch = (y + h).min(viewport.1) - y.max(0.0);
            let share = (cw.max(0.0) * ch.max(0.0)) / viewport_area;
            (share >= COVERING_SHARE).then_some((e, share))
        })
        .collect();
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
        .filter(|e| style(e, "position") == Some("fixed"))
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
    let overlay = covering.iter().find(|c| c.takes_centre).or_else(|| covering.first());
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
            .find(|b| b.top < document_height && b.text == 0 && b.images == 0)
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
