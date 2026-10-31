//! CSS property census (dev-tools T13).
//!
//! For each property in `properties.json`, render one reduced fixture per value
//! plus a control without the declaration, in the engine and in the campaign
//! oracle, and say how far the property gets:
//!
//! - **parse**: the declaration survives into the cascade. `apply_style_property`
//!   (or the logical map) has an arm for the name, or the engine's page changed
//!   when the declaration was added.
//! - **computed**: `getComputedStyle` reads Chrome's value.
//! - **effect**: pixels and geometry match Chrome, on a value that changed
//!   something in Chrome.
//! - **unsupported**: none of the above.
//!
//! A property is its worst value. A value that changes nothing in Chrome proves
//! nothing, so it never reaches effect: see [`Effect::Unexercised`].

use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;

pub const VIEWPORT: (u32, u32) = (400, 300);

/// One line of `properties.json`.
#[derive(Debug, Clone, Deserialize)]
pub struct PropSpec {
    pub name: String,
    pub context: String,
    /// Declarations on `#p` in both arms: what the property needs to show.
    #[serde(default)]
    pub base: String,
    pub values: Vec<String>,
    /// Paints nothing in a static frame (`cursor`, `transition-*`): computed
    /// is all that can be read.
    #[serde(default)]
    pub nonvisual: bool,
    /// Declared on and read from this pseudo-element of `#p`.
    #[serde(default)]
    pub pseudo: Option<String>,
}

pub fn load_spec(json: &str) -> Result<Vec<PropSpec>, String> {
    let spec: Vec<PropSpec> = serde_json::from_str(json).map_err(|e| e.to_string())?;
    match spec.iter().find(|p| p.values.is_empty()) {
        Some(p) => Err(format!("{}: no values", p.name)),
        None => Ok(spec),
    }
}

const SVG_IMAGE: &str = "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='100' height='50'%3E%3Crect width='30' height='50' fill='%23c33'/%3E%3Crect x='30' width='70' height='50' fill='%2339c'/%3E%3C/svg%3E";

/// `(name, css, body)`. `#p` is the element under test. Solid boxes, so a probe
/// is exact; text is large block capitals, so a probe lands inside a stem.
const CONTEXTS: &[(&str, &str, &str)] = &[
    (
        "box",
        "#w{width:300px;height:200px;background:#bbb}#p{width:120px;height:80px;background:#c33}#k{margin-left:40px;width:40px;height:40px;background:#39c}#after{width:60px;height:20px;background:#3a3}",
        "<div id=w><div id=p><div id=k></div></div><div id=after></div></div>",
    ),
    (
        "bg",
        "#w{width:300px;height:200px}#p{width:160px;height:100px;padding:10px;border:10px dashed #888;background-image:linear-gradient(135deg,#c00 50%,#00c 50%);background-size:40px 40px}",
        "<div id=w><div id=p></div></div>",
    ),
    (
        "text",
        "#p{width:320px;font:32px/40px Arial;color:#222}#after{width:60px;height:20px;background:#3a3}",
        "<div id=p>HHH HHH HHH HHH HHH HHH</div><div id=after></div>",
    ),
    (
        "lower",
        "#p{width:320px;font:32px/40px Arial;color:#222}#after{width:60px;height:20px;background:#3a3}",
        "<div id=p>hhh AVAV fi hhh</div><div id=after></div>",
    ),
    (
        "digits",
        "#p{width:320px;font:32px/40px Arial;color:#222}#after{width:60px;height:20px;background:#3a3}",
        "<div id=p><span id=k>1111 1234</span></div><div id=after></div>",
    ),
    (
        "oneline",
        "#p{width:120px;font:32px/40px Arial;color:#222;white-space:nowrap;overflow:hidden}",
        "<div id=p>HHHHHHHHHHHHHHH</div>",
    ),
    (
        "long",
        "#p{width:120px;font:32px/40px Arial;color:#222}#after{width:60px;height:20px;background:#3a3}",
        "<div id=p>HHHHHHHHHHHHHHH HH</div><div id=after></div>",
    ),
    (
        "tabs",
        "#p{width:320px;font:32px/40px Arial;color:#222;white-space:pre}",
        "<div id=p>H\tH</div>",
    ),
    (
        "inline",
        "#w{width:320px;font:32px/60px Arial;color:#222}#p{font-size:16px;background:#c33}",
        "<div id=w>HH<span id=p>HH</span>HH</div>",
    ),
    (
        "quote",
        "#w{width:320px;font:32px/40px Arial;color:#222}",
        "<div id=w><q id=p>H</q></div>",
    ),
    (
        "counter",
        "#w{width:320px;font:32px/40px Arial;color:#222}#k::before{content:counter(c)}",
        "<div id=w><div id=p></div><div id=k></div></div>",
    ),
    (
        "abs",
        "#w{position:relative;width:300px;height:200px;background:#eee}#p{position:absolute;top:0;left:0;width:80px;height:60px;background:#c33}#k{position:absolute;top:10px;left:10px;width:80px;height:60px;background:#39c;z-index:1}",
        "<div id=w><div id=p></div><div id=k></div></div>",
    ),
    (
        "floats",
        "#w{width:300px;font:32px/40px Arial;color:#222}#p{width:80px;height:80px;background:#c33}",
        "<div id=w><div id=p></div>HHH HHH HHH HHH</div>",
    ),
    (
        "cleared",
        "#w{width:300px}#k{float:left;width:80px;height:80px;background:#39c}#p{width:60px;height:40px;background:#c33}",
        "<div id=w><div id=k></div><div id=p></div></div>",
    ),
    (
        "overflow",
        "#p{width:120px;height:80px;background:#ddd}#k{width:200px;height:200px;background:linear-gradient(#c33,#39c)}",
        "<div id=p><div id=k></div></div>",
    ),
    (
        "img",
        "#p{width:200px;height:150px;background:#ddd}",
        "<img id=p src=\"@SVG@\">",
    ),
    (
        "persp",
        "#p{width:200px;height:200px;background:#eee}#k{width:100px;height:100px;background:#c33;transform:rotateY(45deg)}",
        "<div id=p><div id=k></div></div>",
    ),
    (
        "blend",
        "#w{width:300px;height:200px;background:#39c}#p{width:150px;height:100px}#k{width:100px;height:60px;background:#c33;mix-blend-mode:multiply}",
        "<div id=w><div id=p><div id=k></div></div></div>",
    ),
    (
        "input",
        "#p{width:160px;height:30px;font:16px Arial}",
        "<input id=p value=HHH>",
    ),
    (
        "check",
        "#p{width:60px;height:60px;margin:10px}",
        "<input id=p type=checkbox checked>",
    ),
    (
        "flexc",
        "#p{display:flex;width:300px;height:160px;background:#eee}#a{width:60px;height:40px;background:#c33;flex:none}#b{width:60px;height:60px;background:#39c;flex:none}#c{width:60px;height:30px;background:#3a3;flex:none}",
        "<div id=p><div id=a></div><div id=b></div><div id=c></div></div>",
    ),
    (
        "flexi",
        "#w{display:flex;width:300px;height:120px;background:#eee}#p{width:60px;min-width:0;height:50px;background:#c33}#b{width:60px;flex:none;height:80px;background:#39c}",
        "<div id=w><div id=p></div><div id=b></div></div>",
    ),
    (
        "gridc",
        "#p{display:grid;grid-template-columns:60px 60px;grid-template-rows:40px 40px;width:300px;height:160px;background:#eee}#a{background:#c33;width:40px;height:30px}#b{background:#39c;width:40px;height:30px}#c{background:#3a3;width:40px;height:30px}",
        "<div id=p><div id=a></div><div id=b></div><div id=c></div></div>",
    ),
    (
        "gridi",
        "#w{display:grid;grid-template-columns:90px 90px 90px;grid-template-rows:60px 60px 60px;width:270px;background:#eee}#p{background:#c33;min-width:30px;min-height:20px}#b{background:#39c}",
        "<div id=w><div id=p></div><div id=b></div></div>",
    ),
    (
        "list",
        "#p{width:240px;margin:0;padding:0 0 0 60px;font:32px/40px Arial;color:#222}",
        "<ul id=p><li>HH</li><li>HH</li></ul>",
    ),
    (
        "table",
        "#p{border:4px solid #060;width:240px;background:#eee}#p td{border:4px solid #c33;height:30px}#p caption{height:20px;background:#39c}",
        "<table id=p><caption></caption><tr><td id=a>H</td><td></td></tr><tr><td></td><td>H</td></tr></table>",
    ),
    (
        "multicol",
        "#p{width:320px;font:32px/40px Arial;color:#222}",
        "<div id=p>HHH HHH HHH HHH HHH HHH HHH HHH</div>",
    ),
    (
        "spanned",
        "#w{column-count:2;column-gap:20px;width:320px;height:180px}#a,#c{height:60px;background:#39c}#p{height:90px;background:#c33}",
        "<div id=w><div id=a></div><div id=p></div><div id=c></div></div>",
    ),
    (
        "svg",
        "#s{display:block}",
        "<svg id=s width=200 height=120 viewBox=\"0 0 200 120\"><rect id=p x=20 y=20 width=120 height=70></rect></svg>",
    ),
];

pub fn context_exists(name: &str) -> bool {
    CONTEXTS.iter().any(|c| c.0 == name)
}

/// The page for one value, or the control with `None`.
///
/// The control differs only by the declaration's absence, so anything the two
/// frames disagree on is the declaration's doing.
pub fn fixture_html(p: &PropSpec, value: Option<&str>) -> String {
    let (css, body) = CONTEXTS
        .iter()
        .find(|c| c.0 == p.context)
        .map(|c| (c.1, c.2))
        .unwrap_or(("", ""));
    let body = body.replace("@SVG@", SVG_IMAGE);
    let decl = value.map(|v| format!("{}:{v}", p.name)).unwrap_or_default();
    let name = json!(p.name);
    let (rule, read_args) = match &p.pseudo {
        Some(pseudo) => (
            format!("#p{{{}}}#p{pseudo}{{{decl}}}", p.base),
            format!("document.getElementById(\"p\"),{}", json!(pseudo)),
        ),
        // No `;` before a lone declaration: the engine drops the declaration
        // after a leading `;` (see the T13 receipt), and that would read as
        // every property being unsupported.
        None if p.base.is_empty() => (format!("#p{{{decl}}}"), "document.getElementById(\"p\")".to_string()),
        None => (
            format!("#p{{{};{decl}}}", p.base),
            "document.getElementById(\"p\")".to_string(),
        ),
    };
    format!(
        "<!doctype html><html><head><meta charset=utf-8><style>html,body{{margin:0;padding:0;background:#fff}}@keyframes k{{to{{opacity:1}}}}{css}{rule}</style></head><body>{body}<script>(function(){{var c=getComputedStyle({read_args});var o={{c:c.getPropertyValue({name}),r:{{}}}};var a=document.querySelectorAll(\"[id]\");for(var i=0;i<a.length;i++){{var b=a[i].getBoundingClientRect();o.r[a[i].id]=[b.x,b.y,b.width,b.height];}}console.log(\"CENSUS:\"+JSON.stringify(o));}})();</script></body></html>"
    )
}

/// What the page's own script read about itself.
#[derive(Debug, Clone)]
pub struct PageRead {
    pub computed: String,
    pub rects: Vec<(String, [f64; 4])>,
}

/// The `CENSUS:` console line, as both arms' pages print it.
pub fn parse_read(line: &str) -> Option<PageRead> {
    let v: Value = serde_json::from_str(line.strip_prefix("CENSUS:")?).ok()?;
    let computed = v["c"].as_str().unwrap_or("").to_string();
    let rects = v["r"]
        .as_object()?
        .iter()
        .filter_map(|(id, r)| {
            let r = r.as_array()?;
            let n = |i: usize| r.get(i).and_then(Value::as_f64);
            Some((id.clone(), [n(0)?, n(1)?, n(2)?, n(3)?]))
        })
        .collect();
    Some(PageRead { computed, rects })
}

pub fn read_from_event_log(log: &str) -> Option<PageRead> {
    log.lines().find_map(|line| {
        let v: Value = serde_json::from_str(line).ok()?;
        (v["type"] == "console").then(|| parse_read(v["message"].as_str()?)).flatten()
    })
}

#[derive(Debug, Clone)]
pub struct Frame {
    pub width: usize,
    pub height: usize,
    pub rgb: Vec<u8>,
}

/// A binary P6 PPM, as `parity-capture --dump-frame` writes it.
pub fn read_ppm(bytes: &[u8]) -> Option<Frame> {
    let mut at = 0;
    let mut token = || -> Option<String> {
        loop {
            while bytes.get(at)?.is_ascii_whitespace() {
                at += 1;
            }
            if *bytes.get(at)? != b'#' {
                break;
            }
            while *bytes.get(at)? != b'\n' {
                at += 1;
            }
        }
        let start = at;
        while !bytes.get(at)?.is_ascii_whitespace() {
            at += 1;
        }
        Some(String::from_utf8_lossy(&bytes[start..at]).into_owned())
    };
    if token()? != "P6" {
        return None;
    }
    let width: usize = token()?.parse().ok()?;
    let height: usize = token()?.parse().ok()?;
    let _max = token()?;
    let rgb = bytes.get(at + 1..at + 1 + width * height * 3)?.to_vec();
    Some(Frame { width, height, rgb })
}

/// A PNG, as the oracle's screenshot writes it.
pub fn read_png(bytes: &[u8]) -> Option<Frame> {
    let mut decoder = png::Decoder::new(bytes);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let channels = info.color_type.samples();
    if channels < 3 {
        return None;
    }
    let rgb = buf[..info.buffer_size()]
        .chunks_exact(channels)
        .flat_map(|px| [px[0], px[1], px[2]])
        .collect();
    Some(Frame { width: info.width as usize, height: info.height as usize, rgb })
}

/// Probe spacing, in CSS px: small enough that a 6 px border lands on a probe.
pub const PROBE_STEP: usize = 4;
/// Per channel. Colour management and rounding differ by less than this.
pub const PROBE_TOLERANCE: i32 = 16;

fn has_near(f: &Frame, x: usize, y: usize, want: &[u8]) -> bool {
    for ny in y.saturating_sub(1)..=(y + 1).min(f.height - 1) {
        for nx in x.saturating_sub(1)..=(x + 1).min(f.width - 1) {
            let i = (ny * f.width + nx) * 3;
            if (0..3).all(|c| (f.rgb[i + c] as i32 - want[c] as i32).abs() <= PROBE_TOLERANCE) {
                return true;
            }
        }
    }
    false
}

/// How many of the fixed probes disagree between two frames. A probe agrees
/// when either frame's colour is within [`PROBE_TOLERANCE`] somewhere in the
/// other's 3x3 around it: glyph antialiasing and a 1 px seat are not what this
/// census is after, and only the inside of a shape fails both ways.
pub fn probe_diff(a: &Frame, b: &Frame) -> usize {
    if (a.width, a.height) != (b.width, b.height) {
        return (a.width / PROBE_STEP + 1) * (a.height / PROBE_STEP + 1);
    }
    let mut off = 0;
    for y in (0..a.height).step_by(PROBE_STEP) {
        for x in (0..a.width).step_by(PROBE_STEP) {
            let i = (y * a.width + x) * 3;
            if !has_near(b, x, y, &a.rgb[i..i + 3]) && !has_near(a, x, y, &b.rgb[i..i + 3]) {
                off += 1;
            }
        }
    }
    off
}

#[derive(Debug, Clone)]
pub struct Capture {
    pub read: Option<PageRead>,
    pub frame: Option<Frame>,
}

/// One value's four captures.
#[derive(Debug, Clone)]
pub struct Measure {
    pub chrome_value: Capture,
    pub chrome_control: Capture,
    pub rust_value: Capture,
    pub rust_control: Capture,
    pub arm: bool,
    pub nonvisual: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Chrome changed and the engine matches it.
    Match,
    /// Chrome changed and the engine does not match it.
    Mismatch,
    /// Nothing changed in Chrome, so the fixture proved nothing.
    Unexercised,
    /// Listed as painting nothing in a static frame.
    NotVisual,
    /// A capture failed.
    NoCapture,
}

impl Effect {
    pub fn label(self) -> &'static str {
        match self {
            Effect::Match => "match",
            Effect::Mismatch => "mismatch",
            Effect::Unexercised => "unexercised",
            Effect::NotVisual => "not-visual",
            Effect::NoCapture => "no-capture",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Verdict {
    pub parse: bool,
    pub computed: bool,
    pub effect: Effect,
    /// Chrome reads the same with and without the declaration, so a matching
    /// read could be the initial value.
    pub computed_read_vacuous: bool,
}

/// Whitespace-normalised, numbers within 0.05: a resolved width is a float on
/// both sides. An empty read proves nothing, even against an empty one.
pub fn computed_matches(chrome: &str, engine: &str) -> bool {
    let tokens = |s: &str| -> Vec<String> {
        s.replace(',', " , ").replace('(', " ( ").replace(')', " ) ").split_whitespace().map(str::to_string).collect()
    };
    let (a, b) = (tokens(chrome), tokens(engine));
    if a.is_empty() || a.len() != b.len() {
        return false;
    }
    a.iter().zip(&b).all(|(x, y)| x == y || numeric_close(x, y))
}

fn numeric_close(a: &str, b: &str) -> bool {
    fn split(s: &str) -> Option<(f64, &str)> {
        let end = s.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-')).unwrap_or(s.len());
        Some((s[..end].parse().ok()?, &s[end..]))
    }
    matches!((split(a), split(b)), (Some((x, u)), Some((y, v))) if u == v && (x - y).abs() <= 0.05)
}

pub fn geometry_matches(m: &Measure) -> bool {
    match (&m.chrome_value.read, &m.rust_value.read) {
        (Some(a), Some(b)) => rects_match(a, b),
        _ => false,
    }
}

/// One px either way, on every element Chrome reports.
fn rects_match(a: &PageRead, b: &PageRead) -> bool {
    a.rects.iter().all(|(id, ra)| {
        b.rects.iter().find(|(other, _)| other == id).is_some_and(|(_, rb)| {
            ra.iter().zip(rb).all(|(x, y)| (x - y).abs() <= 1.0)
        })
    })
}

/// Every pixel that changed, between two frames of the same engine: no
/// antialiasing to forgive there, so a two-row underline counts in full.
pub fn changed_pixels(a: &Frame, b: &Frame) -> usize {
    if (a.width, a.height) != (b.width, b.height) {
        return a.width * a.height;
    }
    a.rgb
        .chunks_exact(3)
        .zip(b.rgb.chunks_exact(3))
        .filter(|(x, y)| (0..3).any(|c| (x[c] as i32 - y[c] as i32).abs() > PROBE_TOLERANCE))
        .count()
}

/// Three questions. Did the declaration change anything in Chrome? Does the
/// engine's frame agree with Chrome's, allowing the controls' own disagreement
/// plus a tenth of the change (at least 2 probes)? And did the engine change at
/// least half as many pixels? The last one catches what the probes forgive: a
/// thin line the engine never drew.
fn effect_of(value: (&PageRead, &Frame), control: (&PageRead, &Frame), engine: (&PageRead, &Frame), engine_control: &Frame) -> Effect {
    let changed = changed_pixels(value.1, control.1);
    if changed < 3 && rects_match(value.0, control.0) {
        return Effect::Unexercised;
    }
    let floor = probe_diff(control.1, engine_control);
    let off = probe_diff(value.1, engine.1);
    let allowed = floor + (probe_diff(value.1, control.1) / 10).max(2);
    let engine_changed = changed_pixels(engine.1, engine_control);
    if rects_match(value.0, engine.0) && off <= allowed && engine_changed * 2 >= changed {
        Effect::Match
    } else {
        Effect::Mismatch
    }
}

pub fn judge(m: &Measure) -> Verdict {
    let engine_computed = m.rust_value.read.as_ref().map(|r| r.computed.as_str()).unwrap_or("");
    let chrome_computed = m.chrome_value.read.as_ref().map(|r| r.computed.as_str()).unwrap_or("");
    let chrome_control = m.chrome_control.read.as_ref().map(|r| r.computed.as_str()).unwrap_or("");
    let computed = m.rust_value.read.is_some() && computed_matches(chrome_computed, engine_computed);

    let engine_moved = match (&m.rust_value, &m.rust_control) {
        (
            Capture { read: Some(rv), frame: Some(fv) },
            Capture { read: Some(rc), frame: Some(fc) },
        ) => rv.computed != rc.computed || !rects_match(rv, rc) || probe_diff(fv, fc) > 0,
        _ => false,
    };
    let captures = (
        m.chrome_value.read.as_ref().zip(m.chrome_value.frame.as_ref()),
        m.chrome_control.read.as_ref().zip(m.chrome_control.frame.as_ref()),
        m.rust_value.read.as_ref().zip(m.rust_value.frame.as_ref()),
        m.rust_control.frame.as_ref(),
    );
    let effect = if m.nonvisual {
        Effect::NotVisual
    } else if let (Some(v), Some(c), Some(e), Some(ec)) = captures {
        effect_of((v.0, v.1), (c.0, c.1), (e.0, e.1), ec)
    } else {
        Effect::NoCapture
    };
    Verdict {
        parse: m.arm || engine_moved,
        computed,
        effect,
        computed_read_vacuous: chrome_computed == chrome_control,
    }
}

/// Does the census see the difference between the engine's value frame and the
/// engine's control? Run on properties that plainly paint: a "no" means the
/// probes are blind and the run must stop.
pub fn sentinel_sees(m: &Measure) -> bool {
    let mut blind = m.clone();
    blind.rust_value = blind.rust_control.clone();
    judge(m).effect == Effect::Match && judge(&blind).effect == Effect::Mismatch
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Class {
    Unsupported,
    Parse,
    Computed,
    Effect_,
}

impl Class {
    pub fn label(self) -> &'static str {
        match self {
            Class::Unsupported => "unsupported",
            Class::Parse => "parse",
            Class::Computed => "computed",
            Class::Effect_ => "effect",
        }
    }
}

/// A property is its worst measured value. A matching effect is proof the
/// declaration works, so it stands even when `getComputedStyle` does not read
/// the value; that gap shows in the value's own columns. A value that changed
/// nothing in Chrome has nothing to say while a sibling was measured.
pub fn class_of(values: &[Verdict]) -> Class {
    let measured = values.iter().any(|v| matches!(v.effect, Effect::Match | Effect::Mismatch));
    values
        .iter()
        .filter(|v| !(measured && v.effect == Effect::Unexercised))
        .map(|v| match (v.parse, v.computed, v.effect) {
            (_, _, Effect::Match) => Class::Effect_,
            (true, true, _) => Class::Computed,
            (true, false, _) => Class::Parse,
            (false, _, _) => Class::Unsupported,
        })
        .min()
        .unwrap_or(Class::Unsupported)
}

/// What Z's rows should come from: not parsed, or the effect is wrong, or a
/// page-visible read is wrong with no pixels to vouch for it.
pub fn is_red(class: Class, values: &[Verdict]) -> bool {
    match class {
        Class::Unsupported | Class::Parse => true,
        Class::Effect_ => false,
        Class::Computed => values.iter().any(|v| v.effect == Effect::Mismatch),
    }
}

fn fn_body<'a>(src: &'a str, signature: &str) -> Option<&'a str> {
    let at = src.find(signature)?;
    let open = at + src[at..].find('{')?;
    let mut depth = 0usize;
    for (i, c) in src[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&src[open..open + i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// The property names the engine has an arm for, read from the source the same
/// way `scripts/declaration_census.py` does: a line that is a `"name" | "name"
/// =>` arm at the match's own indentation (8 to 12 columns). A deeper match on
/// values is not an arm. The logical map's arms count too.
pub fn extract_arms(src: &str) -> BTreeSet<String> {
    let mut arms = BTreeSet::new();
    for signature in ["fn apply_style_property", "fn logical_to_physical"] {
        let Some(body) = fn_body(src, signature) else { continue };
        for line in body.lines() {
            let indent = line.len() - line.trim_start().len();
            let Some((head, _)) = line.trim().split_once("=>") else { continue };
            if !(8..=12).contains(&indent) || !head.trim_start().starts_with('"') {
                continue;
            }
            for part in head.split('|') {
                let name = part.trim().trim_matches('"');
                if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphabetic() || c == '-') {
                    arms.insert(name.to_ascii_lowercase());
                }
            }
        }
    }
    arms
}

/// A set without the spine, or far too small, means the reader lost the match
/// statement. That is a refusal, never "everything is unsupported".
pub fn check_arms(arms: BTreeSet<String>) -> Result<BTreeSet<String>, String> {
    const REQUIRED: [&str; 7] = ["display", "width", "height", "position", "margin-top", "padding-top", "color"];
    if let Some(missing) = REQUIRED.iter().find(|r| !arms.contains(**r)) {
        return Err(format!("arm reader lost the match: no `{missing}` in {} arms", arms.len()));
    }
    if arms.len() < 50 {
        return Err(format!("arm reader lost the match: only {} arms", arms.len()));
    }
    Ok(arms)
}

/// Which census fields say a loaded site used a property. The technique census
/// asked about these and no others; the rest are "not measured", not zero.
fn census_paths(property: &str) -> &'static [&'static str] {
    match property {
        "position" => &["/stacking/by_reason/position:relative", "/stacking/by_reason/position:absolute", "/stacking/by_reason/position:fixed", "/stacking/by_reason/position:sticky"],
        "z-index" => &["/stacking/by_reason/positioned+z-index", "/stacking/by_reason/flex~1grid-item+z-index"],
        "opacity" => &["/stacking/by_reason/opacity<1"],
        "transform" => &["/stacking/by_reason/transform"],
        "clip-path" => &["/stacking/by_reason/clip-path"],
        "will-change" => &["/stacking/by_reason/will-change"],
        "mask" | "mask-image" | "-webkit-mask-image" => &["/stacking/by_reason/mask", "/icons/mask_image_url/count", "/icons/mask_image_gradient/count"],
        "filter" => &["/stacking/by_reason/filter"],
        "backdrop-filter" => &["/stacking/by_reason/backdrop-filter"],
        "mix-blend-mode" => &["/stacking/by_reason/mix-blend-mode"],
        "isolation" => &["/stacking/by_reason/isolation"],
        "perspective" => &["/stacking/by_reason/perspective"],
        "contain" => &["/stacking/by_reason/contain"],
        "object-fit" => &["/objectFit/by_value"],
        "object-position" => &["/objectFit/non_default_position"],
        "content-visibility" => &["/lazy/content_visibility_auto"],
        _ => &[],
    }
}

/// Top-100 prevalence, from the technique census (`docs/diagnostics/census-top100.json`).
pub struct TopSites {
    pub loaded: usize,
    sites: Vec<Value>,
}

impl TopSites {
    pub fn from_census(census: &Value) -> TopSites {
        let sites: Vec<Value> = census["results"]
            .as_array()
            .map(|all| all.iter().filter(|r| r["status"] == "OK").map(|r| r["data"].clone()).collect())
            .unwrap_or_default();
        TopSites { loaded: sites.len(), sites }
    }

    /// Loaded sites using the property; `None` when the census never asked.
    pub fn sites(&self, property: &str) -> Option<usize> {
        let paths = census_paths(property);
        if paths.is_empty() {
            return None;
        }
        let used = |v: Option<&Value>| match v {
            Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0) > 0.0,
            Some(Value::Object(o)) => !o.is_empty(),
            _ => false,
        };
        Some(self.sites.iter().filter(|site| paths.iter().any(|p| used(site.pointer(p)))).count())
    }
}

/// Declarations of `property` authored in `html`: the name, not inside a longer
/// name, followed by a colon.
pub fn corpus_count(html: &str, property: &str) -> usize {
    let bytes = html.as_bytes();
    html.match_indices(property)
        .filter(|(at, _)| {
            let before = at.checked_sub(1).map(|i| bytes[i] as char);
            let after = html[at + property.len()..].trim_start();
            after.starts_with(':') && !before.is_some_and(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        })
        .count()
}

#[derive(Debug, Clone)]
pub struct ValueRow {
    pub value: String,
    pub parse: bool,
    pub computed: bool,
    pub effect: Effect,
    pub chrome_computed: String,
    pub engine_computed: String,
    pub probes_off: usize,
    pub geometry_ok: bool,
}

#[derive(Debug, Clone)]
pub struct Row {
    pub name: String,
    pub context: String,
    pub sites: Option<usize>,
    pub corpus: usize,
    pub class: Class,
    pub red: bool,
    pub values: Vec<ValueRow>,
}

#[derive(Debug, Clone)]
pub struct Meta {
    pub date: String,
    pub develop_sha: String,
    pub oracle: String,
    pub sites_loaded: usize,
    /// Contexts whose control already disagrees with Chrome, with the worst
    /// count of off probes: the engine's floor on anything measured there.
    pub contexts_off: Vec<(String, usize)>,
}

/// Top-100 sites, then the board corpus, then the name. A property the census
/// never asked about sorts after every property it did.
pub fn sort_rows(rows: &mut [Row]) {
    rows.sort_by(|a, b| {
        b.sites
            .is_some()
            .cmp(&a.sites.is_some())
            .then(b.sites.cmp(&a.sites))
            .then(b.corpus.cmp(&a.corpus))
            .then(a.name.cmp(&b.name))
    });
}

fn count(rows: &[Row], class: Class) -> usize {
    rows.iter().filter(|r| r.class == class).count()
}

fn sites_cell(r: &Row) -> String {
    r.sites.map(|n| n.to_string()).unwrap_or_else(|| "?".into())
}

fn mark(ok: bool) -> &'static str {
    if ok {
        "yes"
    } else {
        "no"
    }
}

fn code(s: &str) -> String {
    format!("`{}`", s.replace('|', "\\|").replace('`', "'"))
}

fn why(r: &Row) -> String {
    r.values
        .iter()
        .filter(|v| !v.computed || !matches!(v.effect, Effect::Match | Effect::NotVisual))
        .map(|v| {
            let read = if v.computed {
                String::new()
            } else if v.engine_computed.is_empty() {
                format!(", reads nothing (Chrome {})", code(&v.chrome_computed))
            } else {
                format!(", reads {} (Chrome {})", code(&v.engine_computed), code(&v.chrome_computed))
            };
            let miss = match v.effect {
                Effect::Mismatch if !v.geometry_ok => format!(" (geometry, {} probes)", v.probes_off),
                Effect::Mismatch => format!(" ({} probes)", v.probes_off),
                _ => String::new(),
            };
            format!("{}: {}{miss}{read}", code(&v.value), v.effect.label())
        })
        .collect::<Vec<_>>()
        .join("; ")
}

pub fn render_markdown(meta: &Meta, rows: &[Row]) -> String {
    let mut md = format!(
        "# CSS property census, {date}\n\n\
         Engine: develop `{sha}`, `parity-capture` at {w}x{h}. Oracle: Chromium {oracle}. Generated by `trench/devtools/tools/css_census`; do not edit by hand.\n\n",
        date = meta.date,
        sha = meta.develop_sha,
        oracle = meta.oracle,
        w = VIEWPORT.0,
        h = VIEWPORT.1,
    );
    md.push_str("A property is its worst value. **effect**: pixels and geometry match Chrome on a value that changed something in Chrome. **computed**: `getComputedStyle` reads Chrome's value. **parse**: the declaration reaches the cascade (an arm in `apply_style_property` or the logical map, or the page moved). **unsupported**: none of these.\n\n");
    md.push_str("| class | properties |\n|---|---:|\n");
    for class in [Class::Effect_, Class::Computed, Class::Parse, Class::Unsupported] {
        md.push_str(&format!("| {} | {} |\n", class.label(), count(rows, class)));
    }
    let red: Vec<&Row> = rows.iter().filter(|r| r.red).collect();
    md.push_str(&format!("| total | {} |\n\n", rows.len()));
    md.push_str("## What this does and does not measure\n\n");
    let measured = rows.iter().filter(|r| r.sites.is_some()).count();
    md.push_str(&format!(
        "- Order: the \"sites\" column counts sites of the {} loaded in the top-100 technique census that use the property. That census asked about only {measured} of these properties; `?` means not measured, not zero, and those rows then sort by declarations authored in the 26-case corpus.\n",
        meta.sites_loaded
    ));
    md.push_str("- One fixture per value in one context. A class says what happened there, not that every value form or context works.\n");
    md.push_str("- An `unexercised` value changed nothing in Chrome, so it cannot pass effect; `not-visual` properties paint nothing in a static frame and stop at computed.\n");
    md.push_str("- An arm that applies a property and is then ignored still counts as parse.\n\n");
    if !meta.contexts_off.is_empty() {
        md.push_str("Controls that already disagree with Chrome (off probes, credited as noise on that context's rows):\n\n");
        for (context, off) in &meta.contexts_off {
            md.push_str(&format!("- {context}: {off}\n"));
        }
        md.push('\n');
    }
    md.push_str(&format!("## Red list ({})\n\n", red.len()));
    md.push_str("| property | sites | corpus | class | why |\n|---|---:|---:|---|---|\n");
    for r in &red {
        md.push_str(&format!("| {} | {} | {} | {} | {} |\n", code(&r.name), sites_cell(r), r.corpus, r.class.label(), why(r)));
    }
    let not_read: Vec<&Row> = rows.iter().filter(|r| r.class == Class::Effect_ && r.values.iter().any(|v| !v.computed)).collect();
    md.push_str(&format!("\n## Paints right, reads wrong ({})\n\nScript on the page sees a different value from Chrome's, though the frame matches.\n\n", not_read.len()));
    for r in &not_read {
        md.push_str(&format!("- {}: {}\n", code(&r.name), why(r)));
    }
    md.push_str("\n## All properties\n\n| property | sites | corpus | class | parse | computed | effect |\n|---|---:|---:|---|---|---|---|\n");
    for r in rows {
        let effects: Vec<&str> = r.values.iter().map(|v| v.effect.label()).collect();
        md.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} |\n",
            code(&r.name),
            sites_cell(r),
            r.corpus,
            r.class.label(),
            mark(r.values.iter().all(|v| v.parse)),
            mark(r.values.iter().all(|v| v.computed)),
            effects.join(", "),
        ));
    }
    md
}

pub fn render_json(meta: &Meta, rows: &[Row]) -> Value {
    json!({
        "meta": {
            "date": meta.date,
            "develop_sha": meta.develop_sha,
            "oracle": meta.oracle,
            "viewport": [VIEWPORT.0, VIEWPORT.1],
            "sites_loaded": meta.sites_loaded,
            "contexts_off": meta.contexts_off,
        },
        "counts": {
            "effect": count(rows, Class::Effect_),
            "computed": count(rows, Class::Computed),
            "parse": count(rows, Class::Parse),
            "unsupported": count(rows, Class::Unsupported),
        },
        "red": rows.iter().filter(|r| r.red).map(|r| r.name.clone()).collect::<Vec<_>>(),
        "properties": rows.iter().map(|r| json!({
            "name": r.name,
            "context": r.context,
            "sites": r.sites,
            "corpus": r.corpus,
            "class": r.class.label(),
            "red": r.red,
            "values": r.values.iter().map(|v| json!({
                "value": v.value,
                "parse": v.parse,
                "computed": v.computed,
                "effect": v.effect.label(),
                "chrome_computed": v.chrome_computed,
                "engine_computed": v.engine_computed,
                "probes_off": v.probes_off,
                "geometry_ok": v.geometry_ok,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}
