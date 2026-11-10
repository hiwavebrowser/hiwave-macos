//! Which image an `<img>` shows (HTML §4.8.4.3, "select an image source").
//!
//! `<picture>`'s `<source>` elements are tried in order (`media`, `type`), then
//! the `srcset` candidates of the winner (or of the `<img>`) are ranked by
//! density against the device pixel ratio, with `w` descriptors turned into
//! densities by the `sizes` attribute. The density of the chosen candidate is
//! returned too: the image's natural size is its pixel size divided by it
//! (`<img srcset="b.png 2x">` with a 200px b.png is 100 CSS px wide).
//!
//! The expected values in the tests were measured in Chromium 143.0.7499.4
//! (Playwright's bundled build; CfT 148 was not to hand) on a page set at an
//! 800x600 viewport with device pixel ratios 1 and 2
//! (docs/diagnostics/2026-11-10/e54b_srcset_chromium.txt).

use rustkit_css::media::media_query_list_matches;

/// What the choice depends on besides the markup.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SourceEnv {
    pub viewport_width: f32,
    pub viewport_height: f32,
    /// `None`: the embedder did not say (the engine has never tracked the window's backing
    /// scale). The densest candidate wins then, which is what the engine did before this and
    /// is right on a dense display; a capture that compares with Chrome at ratio 1 says
    /// `Some(1.0)`.
    pub device_pixel_ratio: Option<f32>,
}

/// The attributes of one `<source>` of a `<picture>`.
#[derive(Debug, Clone, Copy, Default)]
pub struct SourceAttrs<'a> {
    pub srcset: Option<&'a str>,
    pub sizes: Option<&'a str>,
    pub media: Option<&'a str>,
    pub kind: Option<&'a str>,
}

/// The chosen image: its URL as written and its density (a candidate's `x`, or `w / size`).
#[derive(Debug, Clone, PartialEq)]
pub struct Chosen {
    pub url: String,
    pub density: f32,
}

#[derive(Debug, Clone, PartialEq)]
struct Candidate {
    url: String,
    /// `x` descriptor (1 when none) or None when the candidate has a `w` descriptor.
    density: Option<f32>,
    width: Option<f32>,
}

/// Image types the engine can decode (`type` on a `<source>`).
pub fn supports_image_type(kind: &str) -> bool {
    let essence = kind.split(';').next().unwrap_or_default().trim().to_ascii_lowercase();
    matches!(
        essence.as_str(),
        "image/png" | "image/apng" | "image/jpeg" | "image/jpg" | "image/pjpeg" | "image/gif" | "image/webp"
            | "image/bmp" | "image/x-icon" | "image/vnd.microsoft.icon" | "image/svg+xml"
    )
}

fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{c}')
}

/// HTML §4.8.4.2 "parse a srcset attribute", for the descriptors that select: `Nx` and `Nw`.
/// A candidate with a bad or repeated descriptor is dropped, as in the spec.
fn parse_srcset(srcset: &str) -> Vec<Candidate> {
    let mut out = Vec::new();
    let chars: Vec<char> = srcset.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        while i < chars.len() && (is_space(chars[i]) || chars[i] == ',') {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }
        let start = i;
        while i < chars.len() && !is_space(chars[i]) {
            i += 1;
        }
        let mut url: String = chars[start..i].iter().collect();
        let mut descriptors = String::new();
        if url.ends_with(',') {
            // "a.png," : the comma ends the candidate, no descriptors.
            while url.ends_with(',') {
                url.pop();
            }
        } else {
            // Descriptors run to the next comma outside parentheses.
            let mut depth = 0;
            while i < chars.len() {
                let c = chars[i];
                if c == '(' {
                    depth += 1;
                } else if c == ')' && depth > 0 {
                    depth -= 1;
                } else if c == ',' && depth == 0 {
                    break;
                }
                descriptors.push(c);
                i += 1;
            }
        }
        if url.is_empty() {
            continue;
        }
        let mut density: Option<f32> = None;
        let mut width: Option<f32> = None;
        let mut bad = false;
        for token in descriptors.split(is_space).filter(|t| !t.is_empty()) {
            let (number, unit) = token.split_at(token.len() - 1);
            match (unit, number.parse::<f32>()) {
                ("x", Ok(n)) if n > 0.0 && n.is_finite() && density.is_none() && width.is_none() => density = Some(n),
                ("w", Ok(n)) if n > 0.0 && n.fract() == 0.0 && density.is_none() && width.is_none() => width = Some(n),
                _ => bad = true,
            }
        }
        if bad {
            continue;
        }
        out.push(Candidate { url, density: if width.is_some() { None } else { Some(density.unwrap_or(1.0)) }, width });
    }
    out
}

/// A length in `sizes`, in CSS px. Absolute lengths, `vw`/`vh`/`vmin`/`vmax`, `em`/`rem` (16px)
/// and `calc()` sums of those; a negative result or anything else is invalid (None).
fn length_px(value: &str, env: &SourceEnv) -> Option<f32> {
    let v = value.trim();
    if let Some(inner) = v.strip_prefix("calc(").and_then(|r| r.strip_suffix(')')) {
        return calc_px(inner, env);
    }
    let split = v.find(|c: char| c.is_ascii_alphabetic() || c == '%').unwrap_or(v.len());
    let (number, unit) = v.split_at(split);
    let n: f32 = number.trim().parse().ok()?;
    let px = match unit.to_ascii_lowercase().as_str() {
        "px" => n,
        "vw" => n * env.viewport_width / 100.0,
        "vh" => n * env.viewport_height / 100.0,
        "vmin" => n * env.viewport_width.min(env.viewport_height) / 100.0,
        "vmax" => n * env.viewport_width.max(env.viewport_height) / 100.0,
        "em" | "rem" => n * 16.0,
        "pt" => n * 4.0 / 3.0,
        "in" => n * 96.0,
        "cm" => n * 96.0 / 2.54,
        "mm" => n * 96.0 / 25.4,
        "q" => n * 96.0 / 101.6,
        "pc" => n * 16.0,
        "" if n == 0.0 => 0.0,
        _ => return None,
    };
    Some(px)
}

/// `calc()` inside `sizes`: terms joined by ` + ` / ` - `, each a length or a length times
/// (or divided by) a number. No nesting; anything else is invalid.
fn calc_px(expr: &str, env: &SourceEnv) -> Option<f32> {
    let mut total = 0.0f32;
    let mut sign = 1.0f32;
    for token in expr.split_whitespace() {
        match token {
            "+" => sign = 1.0,
            "-" => sign = -1.0,
            term => {
                let value = if let Some((a, b)) = term.split_once('*') {
                    match (length_px(a, env), b.parse::<f32>().ok(), a.parse::<f32>().ok(), length_px(b, env)) {
                        (Some(len), Some(n), _, _) => len * n,
                        (_, _, Some(n), Some(len)) => n * len,
                        _ => return None,
                    }
                } else if let Some((a, b)) = term.split_once('/') {
                    length_px(a, env)? / b.parse::<f32>().ok()?
                } else {
                    length_px(term, env)?
                };
                total += sign * value;
                sign = 1.0;
            }
        }
    }
    Some(total)
}

/// Split at top-level commas (outside parentheses).
fn split_commas(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' if depth > 0 => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&s[start..]);
    parts
}

/// HTML §4.8.4.3 "parse a sizes attribute": the used source size in CSS px. Each entry is an
/// optional media condition and a length; the first entry whose condition holds (an entry with
/// no condition always does) gives the size. No valid entry: `100vw`.
fn source_size(sizes: Option<&str>, env: &SourceEnv) -> f32 {
    let default = env.viewport_width;
    let Some(sizes) = sizes else {
        return default;
    };
    for entry in split_commas(sizes) {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        // The length is the last whitespace-separated component outside parentheses.
        let mut depth = 0;
        let mut cut = None;
        for (i, c) in entry.char_indices() {
            match c {
                '(' => depth += 1,
                ')' if depth > 0 => depth -= 1,
                c if is_space(c) && depth == 0 => cut = Some(i),
                _ => {}
            }
        }
        let (condition, value) = match cut {
            Some(i) => (entry[..i].trim(), entry[i..].trim()),
            None => ("", entry),
        };
        let Some(px) = length_px(value, env).filter(|px| *px >= 0.0) else {
            continue;
        };
        if condition.is_empty() || media_query_list_matches(condition, env.viewport_width, env.viewport_height) {
            return px;
        }
    }
    default
}

/// §4.8.4.3 step "select an image source from a source set" over the parsed candidates.
fn choose(mut candidates: Vec<Candidate>, sizes: Option<&str>, src: Option<&str>, env: &SourceEnv) -> Option<Chosen> {
    // The `src` attribute is a 1x candidate, unless the set is in `w` terms.
    let has_width = candidates.iter().any(|c| c.width.is_some());
    if !has_width {
        if let Some(src) = src.map(str::trim).filter(|s| !s.is_empty()) {
            if !candidates.iter().any(|c| c.url == src) {
                candidates.push(Candidate { url: src.to_string(), density: Some(1.0), width: None });
            }
        }
    }
    if candidates.is_empty() {
        return None;
    }
    let size = source_size(sizes, env).max(f32::MIN_POSITIVE);
    let mut ranked: Vec<(f32, String)> = Vec::new();
    for c in candidates {
        let density = c.density.or_else(|| c.width.map(|w| w / size))?;
        // Later candidates with a density already present are dropped.
        if !ranked.iter().any(|(d, _)| (*d - density).abs() < f32::EPSILON) {
            ranked.push((density, c.url));
        }
    }
    ranked.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let pick = match env.device_pixel_ratio {
        // The smallest candidate at least as dense as the display, else the densest.
        Some(dpr) => ranked.iter().find(|(d, _)| *d >= dpr).or_else(|| ranked.last()),
        None => ranked.last(),
    }?;
    Some(Chosen { url: pick.1.clone(), density: pick.0 })
}

/// The image an `<img>` shows. `sources` are the `<source>` siblings before it, in order, when
/// its parent is a `<picture>` (empty otherwise).
pub fn select_image_source(
    src: Option<&str>,
    srcset: Option<&str>,
    sizes: Option<&str>,
    sources: &[SourceAttrs<'_>],
    env: &SourceEnv,
) -> Option<Chosen> {
    for source in sources {
        let Some(set) = source.srcset else {
            continue;
        };
        if let Some(kind) = source.kind {
            if !supports_image_type(kind) {
                continue;
            }
        }
        if let Some(media) = source.media {
            if !media_query_list_matches(media, env.viewport_width, env.viewport_height) {
                continue;
            }
        }
        let candidates = parse_srcset(set);
        if candidates.is_empty() {
            continue;
        }
        // A `<source>` has no `src` fallback.
        return choose(candidates, source.sizes, None, env);
    }
    choose(parse_srcset(srcset.unwrap_or("")), sizes, src, env)
}

/// The image an `<img>` node shows: its `<picture>`'s earlier `<source>` siblings, then its own
/// `srcset` / `sizes` / `src`.
pub fn select_for_node(node: &rustkit_dom::Node, env: &SourceEnv) -> Option<Chosen> {
    use rustkit_dom::NodeType;
    let NodeType::Element { attributes, .. } = &node.node_type else {
        return None;
    };
    let mut sources: Vec<(Option<String>, Option<String>, Option<String>, Option<String>)> = Vec::new();
    if let Some(parent) = node.parent() {
        if matches!(&parent.node_type, NodeType::Element { tag_name, .. } if tag_name.eq_ignore_ascii_case("picture")) {
            for sibling in parent.children() {
                if sibling.id == node.id {
                    break;
                }
                if let NodeType::Element { tag_name, attributes, .. } = &sibling.node_type {
                    if tag_name.eq_ignore_ascii_case("source") {
                        sources.push((
                            attributes.get("srcset").cloned(),
                            attributes.get("sizes").cloned(),
                            attributes.get("media").cloned(),
                            attributes.get("type").cloned(),
                        ));
                    }
                }
            }
        }
    }
    let borrowed: Vec<SourceAttrs<'_>> = sources
        .iter()
        .map(|(srcset, sizes, media, kind)| SourceAttrs {
            srcset: srcset.as_deref(),
            sizes: sizes.as_deref(),
            media: media.as_deref(),
            kind: kind.as_deref(),
        })
        .collect();
    select_image_source(
        attributes.get("src").map(String::as_str),
        attributes.get("srcset").map(String::as_str),
        attributes.get("sizes").map(String::as_str),
        &borrowed,
        env,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(dpr: f32) -> SourceEnv {
        SourceEnv { viewport_width: 800.0, viewport_height: 600.0, device_pixel_ratio: Some(dpr) }
    }

    /// The expected `currentSrc` and natural width (`pixel width / density`) for the page set,
    /// at each ratio; pixel widths: a=100, b=200, c=400, d=800.
    fn pick(dpr: f32, src: Option<&str>, srcset: &str, sizes: Option<&str>, sources: &[SourceAttrs]) -> (String, f32) {
        let chosen = select_image_source(src, Some(srcset), sizes, sources, &env(dpr)).expect("a source");
        let pixels = match chosen.url.as_str() {
            "a.png" => 100.0,
            "b.png" => 200.0,
            "c.png" => 400.0,
            _ => 800.0,
        };
        (chosen.url, (pixels / chosen.density).round())
    }

    const W4: &str = "a.png 100w, b.png 200w, c.png 400w, d.png 800w";

    #[test]
    fn x_descriptors_follow_the_device_pixel_ratio() {
        let set = "a.png 1x, b.png 2x, c.png 4x";
        assert_eq!(pick(1.0, None, set, None, &[]), ("a.png".into(), 100.0));
        assert_eq!(pick(2.0, None, set, None, &[]), ("b.png".into(), 100.0));
        // 1.5x and 3x: the natural width is the pixel width over the density.
        assert_eq!(pick(1.0, None, "a.png 1.5x, b.png 3x", None, &[]), ("a.png".into(), 67.0));
        assert_eq!(pick(2.0, None, "a.png 1.5x, b.png 3x", None, &[]), ("b.png".into(), 67.0));
        // A lone 2x candidate is chosen at ratio 1 and shown at half its pixel width.
        assert_eq!(pick(1.0, None, "b.png 2x", None, &[]), ("b.png".into(), 100.0));
    }

    #[test]
    fn w_descriptors_use_the_sizes_attribute() {
        assert_eq!(pick(1.0, None, W4, Some("300px"), &[]), ("c.png".into(), 300.0));
        assert_eq!(pick(2.0, None, W4, Some("300px"), &[]), ("d.png".into(), 300.0));
        assert_eq!(pick(1.0, None, W4, Some("50vw"), &[]), ("c.png".into(), 400.0));
        assert_eq!(pick(2.0, None, W4, Some("50vw"), &[]), ("d.png".into(), 400.0));
        assert_eq!(pick(1.0, None, W4, Some("(max-width: 500px) 100px, 600px"), &[]), ("d.png".into(), 600.0));
        assert_eq!(pick(1.0, None, W4, Some("calc(50vw - 100px)"), &[]), ("c.png".into(), 300.0));
        assert_eq!(pick(2.0, None, W4, Some("calc(50vw - 100px)"), &[]), ("d.png".into(), 300.0));
        // No sizes: 100vw.
        assert_eq!(pick(1.0, None, W4, None, &[]), ("d.png".into(), 800.0));
    }

    #[test]
    fn src_is_a_1x_candidate_unless_the_set_uses_w() {
        assert_eq!(pick(1.0, Some("a.png"), "b.png 2x", None, &[]), ("a.png".into(), 100.0));
        assert_eq!(pick(2.0, Some("a.png"), "b.png 2x", None, &[]), ("b.png".into(), 100.0));
        let set = "b.png 200w, c.png 400w";
        assert_eq!(pick(1.0, Some("a.png"), set, Some("200px"), &[]), ("b.png".into(), 200.0));
        assert_eq!(pick(2.0, Some("a.png"), set, Some("200px"), &[]), ("c.png".into(), 200.0));
    }

    #[test]
    fn picture_sources_are_tried_in_order_by_media_and_type() {
        let s = |srcset, media, kind| SourceAttrs { srcset: Some(srcset), sizes: None, media, kind };
        let chosen = |sources: &[SourceAttrs]| select_image_source(Some("c.png"), None, None, sources, &env(1.0)).unwrap().url;
        assert_eq!(chosen(&[s("b.png", Some("(max-width: 900px)"), None)]), "b.png");
        assert_eq!(chosen(&[s("b.png", Some("(max-width: 500px)"), None)]), "c.png");
        assert_eq!(chosen(&[s("b.png", None, Some("image/x-nonsense"))]), "c.png");
        assert_eq!(chosen(&[s("b.png", None, Some("image/png"))]), "b.png");
        assert_eq!(chosen(&[s("a.png", None, None), s("b.png", None, None)]), "a.png");
        // A `type` the engine cannot decode is skipped, so the next source or the img wins.
        assert_eq!(chosen(&[s("d.png", None, Some("image/avif")), s("b.png", None, Some("image/webp"))]), "b.png");
    }

    #[test]
    fn a_picture_source_uses_its_own_sizes() {
        let source = SourceAttrs {
            srcset: Some("a.png 100w, b.png 200w, d.png 800w"),
            sizes: Some("400px"),
            media: Some("(min-width: 400px)"),
            kind: None,
        };
        let chosen = select_image_source(Some("c.png"), None, None, &[source], &env(1.0)).unwrap();
        assert_eq!((chosen.url.as_str(), (800.0 / chosen.density).round()), ("d.png", 400.0));
    }

    #[test]
    fn an_unsized_embedder_gets_the_densest_candidate() {
        let unsized_env = SourceEnv { device_pixel_ratio: None, ..env(1.0) };
        let chosen = select_image_source(None, Some("a.png 1x, b.png 2x, c.png 4x"), None, &[], &unsized_env).unwrap();
        assert_eq!(chosen.url, "c.png");
    }

    #[test]
    fn malformed_candidates_are_dropped_and_an_empty_set_has_no_source() {
        assert_eq!(select_image_source(None, Some(""), None, &[], &env(1.0)), None);
        assert_eq!(select_image_source(None, Some("   "), None, &[], &env(1.0)), None);
        let chosen = select_image_source(None, Some("bad.png 0x, ok.png 2x"), None, &[], &env(1.0)).unwrap();
        assert_eq!(chosen.url, "ok.png");
        // A URL containing a comma stays whole.
        let chosen = select_image_source(None, Some("data:image/png;base64,AAAA 1x"), None, &[], &env(1.0)).unwrap();
        assert_eq!(chosen.url, "data:image/png;base64,AAAA");
    }
}
