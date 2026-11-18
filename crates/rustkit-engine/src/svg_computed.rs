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
//! Cost: nothing at build time. The first read of an svg element's style
//! after a build walks the document's inline svgs once and keeps the
//! answers for that build (`computed_lines_source`).
//!
//! Oracle: Chrome for Testing 148.0.7778.216 through
//! `tools/parity_oracle/svg_computed_paint_log.mjs`, receipt
//! `docs/diagnostics/2026-11-17/svg_computed_paint_cft148.json`.

use crate::SvgPaintStyle;
use rustkit_css::Color;
use rustkit_dom::{Node, NodeType};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

/// What `computed_lines_source` reads svg paint from: the document the
/// view built last and its author paint map (`LayoutCtx::svg_paint_styles`
/// as that build left it).
pub(crate) struct SvgPaintSnapshot {
    pub(crate) root: Weak<Node>,
    pub(crate) paint: Rc<HashMap<usize, SvgPaintStyle>>,
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

/// How many times this thread resolved svg paint (`resolve_all` walks).
pub(crate) fn resolves() -> u64 {
    RESOLVES.with(|c| c.get())
}

const BLACK: Color = Color {
    r: 0,
    g: 0,
    b: 0,
    a: 1.0,
};

/// `value` as a paint, `currentcolor` as `current`; `None` when it does not
/// read (the declaration is then ignored).
pub(crate) fn parse_paint(value: &str, current: Color) -> Option<Paint> {
    let value = value.trim();
    let lower = value.to_ascii_lowercase();
    if lower == "none" {
        return Some(Paint::None);
    }
    if lower == "currentcolor" {
        return Some(Paint::Color(current));
    }
    if lower.starts_with("url(") {
        let close = value.find(')')?;
        let inner = value[4..close].trim();
        let inner = inner
            .strip_prefix(['"', '\''])
            .and_then(|s| s.strip_suffix(['"', '\'']))
            .unwrap_or(inner);
        let rest = value[close + 1..].trim();
        if rest.is_empty() {
            return Some(Paint::Url(inner.to_string(), None));
        }
        return match parse_paint(rest, current)? {
            Paint::Url(..) => None,
            fallback => Some(Paint::Url(inner.to_string(), Some(Box::new(fallback)))),
        };
    }
    crate::parse_color(value).map(Paint::Color)
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

/// The computed `fill` and `stroke` text of every element of every inline
/// `<svg>` under `root`, by raw NodeId. `color_of` is an `<svg>` element's
/// `color` (black when it has none).
pub(crate) fn resolve_all(
    root: &Rc<Node>,
    paint: &HashMap<usize, SvgPaintStyle>,
    color_of: &dyn Fn(usize) -> Option<Color>,
) -> HashMap<usize, (String, String)> {
    fn walk(
        node: &Rc<Node>,
        inherited: Option<(&Paint, &Paint, Color)>,
        paint: &HashMap<usize, SvgPaintStyle>,
        color_of: &dyn Fn(usize) -> Option<Color>,
        out: &mut HashMap<usize, (String, String)>,
    ) {
        let NodeType::Element { tag_name, .. } = &node.node_type else {
            for child in node.children() {
                walk(&child, inherited, paint, color_of, out);
            }
            return;
        };
        let raw = node.id.raw();
        let (fill, stroke, current) = match inherited {
            Some(parent) => parent,
            None if tag_name.eq_ignore_ascii_case("svg") => (
                &Paint::Color(BLACK),
                &Paint::None,
                color_of(raw).unwrap_or(BLACK),
            ),
            None => {
                for child in node.children() {
                    walk(&child, None, paint, color_of, out);
                }
                return;
            }
        };
        let style = paint.get(&raw);
        let fill = resolve(node, style, "fill", fill, current);
        let stroke = resolve(node, style, "stroke", stroke, current);
        out.insert(raw, (fill.css_text(), stroke.css_text()));
        // A `<foreignObject>`'s children are HTML, with their own styles.
        if tag_name.eq_ignore_ascii_case("foreignObject") {
            return;
        }
        for child in node.children() {
            walk(
                &child,
                Some((&fill, &stroke, current)),
                paint,
                color_of,
                out,
            );
        }
    }
    RESOLVES.with(|c| c.set(c.get() + 1));
    let mut out = HashMap::new();
    walk(root, None, paint, color_of, &mut out);
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
