//! `getComputedStyle(el).fill` / `.stroke` for the elements of an inline
//! `<svg>` (A2.1 of the CSS-transitions brief). SVG content gets no
//! `ComputedStyle` and no box, and rustkit-svg resolves paint from the
//! markup at parse time, so nothing in the engine knew an element's
//! computed paint: both read empty. This resolves it from the same inputs
//! the markup is styled with, in the cascade's order (highest first):
//!
//! 1. the element's own `style` attribute, `!important`;
//! 2. author rules, `!important` (`SvgPaintStyle::important`);
//! 3. the own `style` attribute;
//! 4. author rules (`SvgPaintStyle::normal`), or their `inherit` / `unset`
//!    (`SvgPaintStyle::inherit`);
//! 5. the presentation attribute (`fill="blue"`);
//! 6. the parent's value: both properties inherit. Initial: `fill` black,
//!    `stroke` none.
//!
//! A value that does not read as a paint is skipped (Chrome 148 ignores
//! `fill="bogus"` and inherits). `currentcolor` computes to the colour:
//! the `<svg>` element's `color` (its `ComputedStyle`), used for the whole
//! subtree. Serialized as Chrome 148 does: `rgb()` / `rgba()`, `none`,
//! `url("#id")` with its fallback after it.
//!
//! Not modelled (stated gaps): inheritance from HTML ancestors above the
//! `<svg>` (`body { fill: blue }`: the svg starts from the initial
//! values), `color` declared on an element inside the svg, an author
//! `initial` / `revert` (the side cascade leaves those out, so the
//! presentation attribute or the parent's value stands), and an author
//! `inherit !important` ranks as a normal one.
//!
//! Cost: nothing at build time. A read walks up from the element to find
//! its outermost `<svg>` (none: no rows, and nothing else is walked, so a
//! page without inline svg pays only that climb). The first read in an
//! svg resolves that svg's subtree once and keeps the answers until the
//! DOM changes (`rustkit_dom::mutation_generation`), so a presentation
//! attribute written where no relayout follows (a `display: none`
//! subtree) is read at once. Author rules are the last build's.
//!
//! Page input drives no recursion: the subtree walk keeps its own stack,
//! and a paint's fallback is read without re-entering `parse_paint`.
//!
//! Oracle: Chrome for Testing 148.0.7778.216 through
//! `tools/parity_oracle/svg_computed_paint_log.mjs`, receipt
//! `docs/diagnostics/2026-11-17/svg_computed_paint_cft148.json`.

use crate::SvgPaintStyle;
use rustkit_css::Color;
use rustkit_dom::{Document, Node, NodeId, NodeType};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

/// What `computed_lines_source` reads svg paint from: the document the
/// view built last and its author paint map (`LayoutCtx::svg_paint_styles`
/// as that build left it).
pub(crate) struct SvgPaintSnapshot {
    pub(crate) document: Weak<Document>,
    pub(crate) paint: Rc<HashMap<usize, SvgPaintStyle>>,
    /// Per outermost `<svg>` (raw NodeId): its elements' fill and stroke
    /// text, at the DOM generation they were resolved at.
    resolved: RefCell<(u64, HashMap<usize, Rc<HashMap<usize, (String, String)>>>)>,
}

impl SvgPaintSnapshot {
    pub(crate) fn new(document: &Rc<Document>, paint: Rc<HashMap<usize, SvgPaintStyle>>) -> Self {
        Self {
            document: Rc::downgrade(document),
            paint,
            resolved: RefCell::default(),
        }
    }

    /// `node`'s computed fill and stroke text when it is an element of an
    /// inline `<svg>` (the `<svg>` included); `color_of` is an `<svg>`
    /// element's `color`.
    pub(crate) fn fill_stroke(
        &self,
        node: usize,
        color_of: &dyn Fn(usize) -> Option<Color>,
    ) -> Option<(String, String)> {
        let document = self.document.upgrade()?;
        let node = document.get_node(NodeId::new(node))?;
        let svg = outermost_svg(&node)?;
        let gen = rustkit_dom::mutation_generation();
        let mut resolved = self.resolved.borrow_mut();
        if resolved.0 != gen {
            *resolved = (gen, HashMap::new());
        }
        let raw = svg.id.raw();
        let answers = resolved
            .1
            .entry(raw)
            .or_insert_with(|| {
                Rc::new(resolve_svg(
                    &svg,
                    &self.paint,
                    color_of(raw).unwrap_or(BLACK),
                ))
            })
            .clone();
        answers.get(&node.id.raw()).cloned()
    }
}

fn is_tag(node: &Node, tag: &str) -> bool {
    node.tag_name().is_some_and(|t| t.eq_ignore_ascii_case(tag))
}

/// The outermost `<svg>` at or above `node` whose content `node` is: the
/// climb stops at a `<foreignObject>` above `node` (its HTML children are
/// not svg content). `None` for anything else.
fn outermost_svg(node: &Rc<Node>) -> Option<Rc<Node>> {
    if !matches!(node.node_type, NodeType::Element { .. }) {
        return None;
    }
    let mut svg = is_tag(node, "svg").then(|| node.clone());
    let mut at = node.parent();
    while let Some(n) = at {
        if is_tag(&n, "foreignObject") {
            break;
        }
        if is_tag(&n, "svg") {
            svg = Some(n.clone());
        }
        at = n.parent();
    }
    svg
}

/// A computed `<paint>` (SVG 2 §13.2): `currentcolor` already resolved.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Paint {
    None,
    Color(Color),
    /// A paint server and its fallback (`none` or a colour).
    Url(String, Option<Box<Paint>>),
}

impl Paint {
    pub(crate) fn css_text(&self) -> String {
        match self {
            Paint::None => "none".to_string(),
            Paint::Color(c) => crate::css_color(*c),
            Paint::Url(url, None) => format!("url(\"{url}\")"),
            Paint::Url(url, Some(fallback)) => format!("url(\"{url}\") {}", fallback.css_text()),
        }
    }
}

thread_local! {
    static RESOLVES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// How many times this thread resolved svg paint (`resolve_svg` walks).
#[cfg(test)]
pub(crate) fn resolves() -> u64 {
    RESOLVES.with(|c| c.get())
}

const BLACK: Color = Color {
    r: 0,
    g: 0,
    b: 0,
    a: 1.0,
};

fn starts_with_url(value: &str) -> bool {
    value
        .get(..4)
        .is_some_and(|p| p.eq_ignore_ascii_case("url("))
}

/// `none`, `currentcolor` (as `current`) or a colour: what a paint server's
/// fallback may be.
fn parse_simple_paint(value: &str, current: Color) -> Option<Paint> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("none") {
        return Some(Paint::None);
    }
    if value.eq_ignore_ascii_case("currentcolor") {
        return Some(Paint::Color(current));
    }
    crate::parse_color(value).map(Paint::Color)
}

/// A leading `url(...)`: its address and what follows it. A quoted address
/// may hold `)` (`url("a)b")`).
fn split_url(value: &str) -> Option<(&str, &str)> {
    let body = value.get(4..)?.trim_start();
    match body.chars().next()? {
        quote @ ('"' | '\'') => {
            let end = body[1..].find(quote)? + 1;
            let rest = body[end + 1..].trim_start().strip_prefix(')')?;
            Some((&body[1..end], rest))
        }
        _ => {
            let end = body.find(')')?;
            Some((body[..end].trim(), &body[end + 1..]))
        }
    }
}

/// `value` as a paint, `currentcolor` as `current`; `None` when it does not
/// read (the declaration is then ignored). A paint server's fallback is
/// read on its own: a second `url()` is invalid at once.
pub(crate) fn parse_paint(value: &str, current: Color) -> Option<Paint> {
    let value = value.trim();
    if !starts_with_url(value) {
        return parse_simple_paint(value, current);
    }
    let (url, rest) = split_url(value)?;
    let rest = rest.trim();
    if rest.is_empty() {
        return Some(Paint::Url(url.to_string(), None));
    }
    if starts_with_url(rest) {
        return None;
    }
    let fallback = parse_simple_paint(rest, current)?;
    Some(Paint::Url(url.to_string(), Some(Box::new(fallback))))
}

/// The last `property` declaration of `text` (`name:value;` pairs, as
/// `SvgPaintStyle` keeps them, or a `style` attribute), with whether it is
/// `!important` (only a `style` attribute carries the flag in its text).
fn declared<'a>(text: &'a str, property: &str) -> Vec<(&'a str, bool)> {
    text.split(';')
        .filter_map(|decl| {
            let (name, value) = decl.split_once(':')?;
            name.trim()
                .eq_ignore_ascii_case(property)
                .then(|| crate::split_important(value))
        })
        .collect()
}

/// `property` of an element: `inherited` is its parent's value.
fn resolve(
    node: &Node,
    style: Option<&SvgPaintStyle>,
    property: &str,
    inherited: &Paint,
    current: Color,
) -> Paint {
    let own = node.get_attribute("style").unwrap_or("");
    let own = declared(own, property);
    let read = |value: &str| -> Option<Paint> {
        match value.trim().to_ascii_lowercase().as_str() {
            "inherit" | "unset" => Some(inherited.clone()),
            "initial" => Some(match property {
                "fill" => Paint::Color(BLACK),
                _ => Paint::None,
            }),
            _ => parse_paint(value, current),
        }
    };
    let last = |list: &[(&str, bool)], important: bool| {
        list.iter()
            .rev()
            .filter(|(_, i)| *i == important)
            .find_map(|(v, _)| read(v))
    };
    let author = |text: &str| {
        declared(text, property)
            .iter()
            .rev()
            .find_map(|(v, _)| read(v))
    };
    last(&own, true)
        .or_else(|| style.and_then(|s| author(&s.important)))
        .or_else(|| last(&own, false))
        .or_else(|| style.and_then(|s| author(&s.normal)))
        .or_else(|| {
            style
                .is_some_and(|s| s.inherit.iter().any(|p| p == property))
                .then(|| inherited.clone())
        })
        .or_else(|| {
            node.get_attribute(property)
                .and_then(|v| parse_paint(v, current))
        })
        .unwrap_or_else(|| inherited.clone())
}

/// The computed `fill` and `stroke` text of every element of the inline
/// `<svg>` `svg`, by raw NodeId, its `currentcolor` `current`. The walk
/// keeps its own stack (a page's nesting depth drives no recursion).
fn resolve_svg(
    svg: &Rc<Node>,
    paint: &HashMap<usize, SvgPaintStyle>,
    current: Color,
) -> HashMap<usize, (String, String)> {
    RESOLVES.with(|c| c.set(c.get() + 1));
    let mut out = HashMap::new();
    let mut stack: Vec<(Rc<Node>, Rc<(Paint, Paint)>)> =
        vec![(svg.clone(), Rc::new((Paint::Color(BLACK), Paint::None)))];
    while let Some((node, parent)) = stack.pop() {
        if !matches!(node.node_type, NodeType::Element { .. }) {
            continue;
        }
        let style = paint.get(&node.id.raw());
        let fill = resolve(&node, style, "fill", &parent.0, current);
        let stroke = resolve(&node, style, "stroke", &parent.1, current);
        out.insert(node.id.raw(), (fill.css_text(), stroke.css_text()));
        // A `<foreignObject>`'s children are HTML, with their own styles.
        if is_tag(&node, "foreignObject") {
            continue;
        }
        let values = Rc::new((fill, stroke));
        for child in node.children() {
            stack.push((child, values.clone()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paints_read_and_serialize_as_chrome() {
        let blue = Color {
            r: 0,
            g: 0,
            b: 255,
            a: 1.0,
        };
        let text = |v: &str| parse_paint(v, blue).map(|p| p.css_text());
        assert_eq!(text("none").as_deref(), Some("none"));
        assert_eq!(text("currentColor").as_deref(), Some("rgb(0, 0, 255)"));
        assert_eq!(text("url(#gr)").as_deref(), Some("url(\"#gr\")"));
        assert_eq!(
            text("url('#gr') red").as_deref(),
            Some("url(\"#gr\") rgb(255, 0, 0)")
        );
        assert_eq!(text("url(#gr) none").as_deref(), Some("url(\"#gr\") none"));
        assert_eq!(text("url(#a) url(#b)"), None);
        assert_eq!(
            text("url(\"a)b\") red").as_deref(),
            Some("url(\"a)b\") rgb(255, 0, 0)")
        );
        assert_eq!(text("url(\"a"), None);
        assert_eq!(text("coral").as_deref(), Some("rgb(255, 127, 80)"));
        assert_eq!(text("bogus"), None);
    }

    /// R1 on #1294: page input must not drive recursion. 100k `url()`s
    /// overflowed the stack, one frame per `url()`.
    #[test]
    fn a_long_url_chain_is_rejected() {
        let chain = "url(a)".repeat(100_000);
        assert_eq!(parse_paint(&chain, BLACK), None);
    }
}
