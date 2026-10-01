"""Queue item 3: engine parse arms, shorthand parser and display-list JSON for elliptical corners.
One-shot source edit on the rs-elliptical-corners worktree; asserts every anchor matches once."""
p = '/Users/petecopeland/Repos/.worktrees/rs-control-semantics/crates/rustkit-engine/src/lib.rs'
s = open(p, newline='').read()


def rep(old, new, count=1):
    global s
    assert s.count(old) == count, (s.count(old), old[:80])
    s = s.replace(old, new)


# ---- parse arms
a = s.index('''            "border-radius" => {
                // 1–4 values in box order''')
b = s.index('''            "box-shadow" => {
                // Parse box-shadow: offset-x offset-y blur spread color [inset]''')
new = '''            "border-radius" => {
                // 1–4 values in box order: top-left, top-right,
                // bottom-right, bottom-left (CSS Backgrounds 3 §5.1). Only
                // the one-value form used to parse; `8px 8px 0 0` (the
                // top-rounded card/tab idiom) failed parse_length and the
                // whole declaration was dropped, leaving square corners.
                // `h / v` gives each corner a horizontal and a vertical
                // radius, each side of the slash expanded by the same 1–4
                // rule.
                if let Some([tl, tr, br, bl]) = parse_border_radius_shorthand(value) {
                    style.border_top_left_radius = tl;
                    style.border_top_right_radius = tr;
                    style.border_bottom_right_radius = br;
                    style.border_bottom_left_radius = bl;
                }
            }
            // The flow-relative names map onto the physical corners of a
            // horizontal-tb, left-to-right box (css-logical-1 §4.5); other
            // writing modes are not mapped.
            "border-top-left-radius" | "border-start-start-radius" => {
                if let Some(corner) = parse_corner_radius(value) {
                    style.border_top_left_radius = corner;
                }
            }
            "border-top-right-radius" | "border-start-end-radius" => {
                if let Some(corner) = parse_corner_radius(value) {
                    style.border_top_right_radius = corner;
                }
            }
            "border-bottom-right-radius" | "border-end-end-radius" => {
                if let Some(corner) = parse_corner_radius(value) {
                    style.border_bottom_right_radius = corner;
                }
            }
            "border-bottom-left-radius" | "border-end-start-radius" => {
                if let Some(corner) = parse_corner_radius(value) {
                    style.border_bottom_left_radius = corner;
                }
            }
'''
s = s[:a] + new + s[b:]

# ---- shorthand parser
a = s.index('/// `border-radius` shorthand without the `/` part: 1–4 lengths expanded to')
b = s.index('/// Parse a box-shadow value from CSS.')
new = '''/// Split a declaration value at top-level whitespace and `/`, keeping
/// anything inside parentheses whole (`calc(1px / 2)` is one token). The
/// slash comes back as its own `"/"` token.
fn split_radius_tokens(value: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut depth = 0usize;
    let mut start = None;
    for (i, ch) in value.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ => {}
        }
        let boundary = depth == 0 && (ch.is_whitespace() || ch == '/');
        if boundary {
            if let Some(from) = start.take() {
                tokens.push(&value[from..i]);
            }
            if ch == '/' {
                tokens.push(&value[i..i + 1]);
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(from) = start {
        tokens.push(&value[from..]);
    }
    tokens
}

/// 1–4 radii expanded to `[top-left, top-right, bottom-right, bottom-left]`
/// (CSS Backgrounds 3 §5.1). None for a bad token, no value, or more than
/// four. A negative radius is invalid.
fn expand_radius_values(tokens: &[&str]) -> Option<[rustkit_css::Length; 4]> {
    let v: Vec<rustkit_css::Length> = tokens
        .iter()
        .map(|t| rustkit_css::parse_length(t))
        .collect::<Option<_>>()?;
    if v.iter().any(|l| matches!(l, rustkit_css::Length::Px(px) if *px < 0.0)) {
        return None;
    }
    Some(match v.as_slice() {
        [a] => [a.clone(), a.clone(), a.clone(), a.clone()],
        [a, b] => [a.clone(), b.clone(), a.clone(), b.clone()],
        [a, b, c] => [a.clone(), b.clone(), c.clone(), b.clone()],
        [a, b, c, d] => [a.clone(), b.clone(), c.clone(), d.clone()],
        _ => return None,
    })
}

/// The `border-radius` shorthand: 1–4 horizontal radii, then optionally `/`
/// and 1–4 vertical radii, as `[top-left, top-right, bottom-right,
/// bottom-left]`. Without a slash each corner's vertical radius is its
/// horizontal one. None for anything it cannot read whole.
fn parse_border_radius_shorthand(value: &str) -> Option<[rustkit_css::CornerRadius; 4]> {
    let tokens = split_radius_tokens(value);
    let mut halves = tokens.split(|t| *t == "/");
    let horizontal = expand_radius_values(halves.next()?)?;
    let vertical = match halves.next() {
        Some(half) => expand_radius_values(half)?,
        None => horizontal.clone(),
    };
    if halves.next().is_some() {
        return None;
    }
    let [h0, h1, h2, h3] = horizontal;
    let [v0, v1, v2, v3] = vertical;
    let corner = |horizontal, vertical| rustkit_css::CornerRadius { horizontal, vertical };
    Some([corner(h0, v0), corner(h1, v1), corner(h2, v2), corner(h3, v3)])
}

/// One corner's longhand (`border-top-left-radius`): one radius for both
/// axes, or a horizontal then a vertical one. There is no slash here.
fn parse_corner_radius(value: &str) -> Option<rustkit_css::CornerRadius> {
    let tokens = split_radius_tokens(value);
    let (horizontal, vertical) = match tokens.as_slice() {
        [both] => (*both, *both),
        [horizontal, vertical] => (*horizontal, *vertical),
        _ => return None,
    };
    let [horizontal, vertical, ..] = expand_radius_values(&[horizontal, vertical])?;
    Some(rustkit_css::CornerRadius { horizontal, vertical })
}

'''
s = s[:a] + new + s[b:]

# ---- display-list JSON: the scalar keys stay (horizontal radii); the vertical ones are added
rep('''            serde_json::json!({
                "top_left": r.top_left,
                "top_right": r.top_right,
                "bottom_right": r.bottom_right,
                "bottom_left": r.bottom_left
            })''', '''            // The four scalar keys are the horizontal radii, as before radii
            // had two axes; `vertical` carries the other axis.
            serde_json::json!({
                "top_left": r.top_left.h,
                "top_right": r.top_right.h,
                "bottom_right": r.bottom_right.h,
                "bottom_left": r.bottom_left.h,
                "vertical": {
                    "top_left": r.top_left.v,
                    "top_right": r.top_right.v,
                    "bottom_right": r.bottom_right.v,
                    "bottom_left": r.bottom_left.v
                }
            })''')

# ---- tests: the shorthand pin
a = s.index('    fn border_radius_shorthand_expands_one_to_four_values() {')
b = s.index('\n    }\n', a) + len('\n    }\n')
new = '''    fn border_radius_shorthand_expands_one_to_four_values() {
        use rustkit_css::Length::{Percent, Px, Zero};
        let circular = |value: &str| {
            parse_border_radius_shorthand(value).map(|corners| {
                corners.map(|c| {
                    assert_eq!(c.horizontal, c.vertical, "{value}: no slash, one radius per corner");
                    c.horizontal
                })
            })
        };
        assert_eq!(circular("8px"), Some([Px(8.0), Px(8.0), Px(8.0), Px(8.0)]));
        assert_eq!(
            circular("8px 8px 0 0"),
            Some([Px(8.0), Px(8.0), Zero, Zero])
        );
        assert_eq!(
            circular("1px 2px"),
            Some([Px(1.0), Px(2.0), Px(1.0), Px(2.0)])
        );
        assert_eq!(
            circular("1px 2px 3px"),
            Some([Px(1.0), Px(2.0), Px(3.0), Px(2.0)])
        );
        assert_eq!(parse_border_radius_shorthand("1px 2px 3px 4px 5px"), None);
        assert_eq!(parse_border_radius_shorthand("8px bogus"), None);
        assert_eq!(parse_border_radius_shorthand("-8px"), None);

        // `h / v`: each side of the slash expands on its own.
        let pairs = |value: &str| {
            parse_border_radius_shorthand(value)
                .map(|corners| corners.map(|c| (c.horizontal, c.vertical)))
        };
        assert_eq!(
            pairs("50px / 25px"),
            Some([
                (Px(50.0), Px(25.0)),
                (Px(50.0), Px(25.0)),
                (Px(50.0), Px(25.0)),
                (Px(50.0), Px(25.0)),
            ])
        );
        assert_eq!(
            pairs("1px 2px 3px 4px/50% 6px"),
            Some([
                (Px(1.0), Percent(50.0)),
                (Px(2.0), Px(6.0)),
                (Px(3.0), Percent(50.0)),
                (Px(4.0), Px(6.0)),
            ])
        );
        assert_eq!(parse_border_radius_shorthand("1px / 2px / 3px"), None);
        assert_eq!(parse_border_radius_shorthand("1px /"), None);
        assert_eq!(parse_border_radius_shorthand("/ 1px"), None);
    }

    #[test]
    fn a_corner_longhand_takes_one_or_two_radii() {
        use rustkit_css::Length::{Percent, Px};
        let pair = |value: &str| parse_corner_radius(value).map(|c| (c.horizontal, c.vertical));
        assert_eq!(pair("10px"), Some((Px(10.0), Px(10.0))));
        assert_eq!(pair("10px 20%"), Some((Px(10.0), Percent(20.0))));
        assert_eq!(pair("10px 20px 30px"), None);
        assert_eq!(pair("10px / 20px"), None);
        assert_eq!(pair(""), None);
    }
'''
s = s[:a] + new + s[b:]

# ---- tests: the horizontal-half pin becomes the elliptical pins
a = s.index('''    /// Elliptical radii take the horizontal half rather than failing to
    /// parse.''')
b = s.index('    // ── box-shadow paint order (#74) ──')
new = '''    /// The radius of the one rounded rect a 200x100 box with `style_decls`
    /// paints, as the display list prints it.
    fn painted_radius(e: &Engine, style_decls: &str) -> String {
        let html = format!(
            r#"<!DOCTYPE html><html><body><div style="width:200px;height:100px;background-color:#3366cc;font-size:10px;{style_decls}"></div></body></html>"#
        );
        let list = dl(e, &html);
        let line = list
            .lines()
            .find(|c| c.starts_with("RoundedRect"))
            .unwrap_or_else(|| panic!("no RoundedRect for `{style_decls}`:\\n{list}"));
        line[line.find("radius:").expect("radius field")..].to_string()
    }

    /// `h / v` reaches paint as an ellipse per corner. This used to keep the
    /// horizontal half and paint a 10px circle.
    #[test]
    fn an_elliptical_radius_reaches_paint_with_both_axes() {
        let e = engine();
        assert_eq!(rounded_rect_count(&e, "border-radius:10px / 20px"), 1);
        let radius = painted_radius(&e, "border-radius:10px / 20px");
        assert_eq!(
            radius.matches("CornerRadius { h: 10.0, v: 20.0 }").count(),
            4,
            "{radius}"
        );
    }

    /// A longhand with two values is one corner's horizontal then vertical
    /// radius; the whole declaration used to be dropped.
    #[test]
    fn a_two_value_corner_longhand_rounds_that_corner() {
        let e = engine();
        let radius = painted_radius(&e, "border-top-left-radius:30px 15px");
        assert!(radius.contains("top_left: CornerRadius { h: 30.0, v: 15.0 }"), "{radius}");
        assert!(radius.contains("top_right: CornerRadius { h: 0.0, v: 0.0 }"), "{radius}");
    }

    /// The flow-relative longhands land on the physical corners of an
    /// ltr horizontal box.
    #[test]
    fn a_logical_corner_longhand_rounds_its_physical_corner() {
        let e = engine();
        let radius = painted_radius(&e, "border-end-end-radius:12px");
        assert!(radius.contains("bottom_right: CornerRadius { h: 12.0, v: 12.0 }"), "{radius}");
        let radius = painted_radius(&e, "border-start-end-radius:12px");
        assert!(radius.contains("top_right: CornerRadius { h: 12.0, v: 12.0 }"), "{radius}");
    }

    /// A percentage is of the width for the horizontal radius and of the
    /// height for the vertical one: `50%` on 200x100 is a 100x50 ellipse,
    /// not a 50px pill end.
    #[test]
    fn a_percentage_radius_resolves_per_axis() {
        let e = engine();
        let radius = painted_radius(&e, "border-radius:50%");
        assert_eq!(
            radius.matches("CornerRadius { h: 100.0, v: 50.0 }").count(),
            4,
            "{radius}"
        );
        let radius = painted_radius(&e, "border-radius:10% / 30%");
        assert_eq!(
            radius.matches("CornerRadius { h: 20.0, v: 30.0 }").count(),
            4,
            "{radius}"
        );
    }

    /// Radii that overlap are all scaled by one factor (CSS Backgrounds 3
    /// §5.5), so corners keep their proportions. Two 150px top corners on a
    /// 200px-wide box scale by 200/300; they are not each cut to half the
    /// shorter side.
    #[test]
    fn overlapping_radii_are_reduced_by_one_factor() {
        let e = engine();
        let radius = painted_radius(&e, "border-radius:150px 150px 30px 30px");
        assert!(radius.contains("top_left: CornerRadius { h: 100.0, v: 100.0 }"), "{radius}");
        assert!(radius.contains("bottom_left: CornerRadius { h: 20.0, v: 20.0 }"), "{radius}");

        // Top-only radii as tall as the box stay whole: nothing on the left
        // or right side overlaps them.
        let radius = painted_radius(&e, "border-radius:100px 100px 0 0");
        assert!(radius.contains("top_left: CornerRadius { h: 100.0, v: 100.0 }"), "{radius}");

        // The pill: a huge radius is half the shorter side.
        let radius = painted_radius(&e, "border-radius:9999px");
        assert_eq!(
            radius.matches("CornerRadius { h: 50.0, v: 50.0 }").count(),
            4,
            "{radius}"
        );
    }

'''
s = s[:a] + new + s[b:]
open(p, 'w', newline='').write(s)
print('ok')
