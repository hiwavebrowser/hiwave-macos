//! `rgb()` / `rgba()` / `hsl()` / `hsla()` in both CSS Color 4 forms:
//!
//! - modern: `rgb(70 31 140 / 60%)` — space-separated channels, an optional
//!   `/ alpha`, numbers and percentages mixed freely, `none` for 0;
//! - legacy: `rgba(70, 31, 140, 0.6)` — commas, no `/`, no `none`, and the
//!   three rgb channels all numbers or all percentages.
//!
//! Mixing the two (`rgb(1, 2 3)`) is invalid, and an invalid colour must
//! drop the declaration. Channels clamp to 0..=255 and round; alpha clamps
//! to 0..=1. `calc()` over numbers and percentages works in any slot.
//! The checked answers are Chromium 143's (see `color_syntax_tests.rs`).

use crate::Color;

/// One argument slot's value.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Arg {
    Number(f32),
    Percent(f32),
    /// An angle in degrees (hue only).
    Degrees(f32),
    None,
}

/// Parse a lowercase `rgb(`/`rgba(`/`hsl(`/`hsla(` function, or `None`.
pub(crate) fn parse_color_function(value: &str) -> Option<Color> {
    let (is_hsl, rest) = if let Some(r) = value.strip_prefix("rgba(") {
        (false, r)
    } else if let Some(r) = value.strip_prefix("rgb(") {
        (false, r)
    } else if let Some(r) = value.strip_prefix("hsla(") {
        (true, r)
    } else if let Some(r) = value.strip_prefix("hsl(") {
        (true, r)
    } else {
        return None;
    };
    // An unclosed function at the end of a value closes itself.
    let inner = rest.strip_suffix(')').unwrap_or(rest);
    let tokens = tokenize(inner)?;

    let legacy = tokens.iter().any(|t| matches!(t, Tok::Comma));
    let (args, alpha) = if legacy {
        // a , b , c [, alpha]
        let mut args = Vec::new();
        for (i, t) in tokens.iter().enumerate() {
            match (i % 2, t) {
                (0, Tok::Arg(s)) => args.push(*s),
                (1, Tok::Comma) => {}
                _ => return None,
            }
        }
        if tokens.len() % 2 == 0 || !(3..=4).contains(&args.len()) {
            return None;
        }
        (args[..3].to_vec(), args.get(3).copied())
    } else {
        // a b c [/ alpha]
        let mut args = Vec::new();
        let mut alpha = None;
        let mut slash = false;
        for t in &tokens {
            match t {
                Tok::Arg(s) if !slash => args.push(*s),
                Tok::Arg(s) if alpha.is_none() => alpha = Some(*s),
                Tok::Slash if !slash => slash = true,
                _ => return None,
            }
        }
        if args.len() != 3 || (slash && alpha.is_none()) {
            return None;
        }
        (args, alpha)
    };

    let arg = |s: &str| {
        let a = parse_arg(s)?;
        // `none` is modern-syntax only.
        if legacy && a == Arg::None {
            return None;
        }
        Some(a)
    };
    let alpha = match alpha {
        None => 1.0,
        Some(s) => match arg(s)? {
            Arg::Number(n) => n,
            Arg::Percent(p) => p / 100.0,
            Arg::None => 0.0,
            Arg::Degrees(_) => return None,
        }
        .clamp(0.0, 1.0),
    };

    if is_hsl {
        let hue = match arg(args[0])? {
            Arg::Number(n) | Arg::Degrees(n) => n as f64,
            Arg::None => 0.0,
            Arg::Percent(_) => return None,
        };
        // Saturation and lightness: percentages, or (modern, and as the old
        // comma parser also allowed) bare numbers meaning the same.
        let pct = |s: &str| match arg(s)? {
            // In f64: `90%` as f32 / 100 is 0.89999998, a hair under the .5
            // that Chromium rounds up (230 for hsl(0 0% 90%)).
            Arg::Percent(p) | Arg::Number(p) => Some(p as f64 / 100.0),
            Arg::None => Some(0.0),
            Arg::Degrees(_) => None,
        };
        let (s, l) = (pct(args[1])?, pct(args[2])?);
        let (r, g, b) = crate::hsl_to_rgb(hue, s, l);
        return Some(Color::new(r, g, b, alpha));
    }

    let channels = [arg(args[0])?, arg(args[1])?, arg(args[2])?];
    if legacy {
        let all_pct = channels.iter().all(|c| matches!(c, Arg::Percent(_)));
        let all_num = channels.iter().all(|c| matches!(c, Arg::Number(_)));
        if !all_pct && !all_num {
            return None;
        }
    }
    let mut rgb = [0u8; 3];
    for (out, c) in rgb.iter_mut().zip(channels) {
        let v = match c {
            Arg::Number(n) => n,
            Arg::Percent(p) => p * 255.0 / 100.0,
            Arg::None => 0.0,
            Arg::Degrees(_) => return None,
        };
        *out = v.clamp(0.0, 255.0).round() as u8;
    }
    Some(Color::new(rgb[0], rgb[1], rgb[2], alpha))
}

#[derive(Debug)]
enum Tok<'a> {
    Arg(&'a str),
    Comma,
    Slash,
}

/// Split the function's inside into arguments and `,` / `/` separators,
/// keeping nested `( … )` (a `calc()`) whole.
fn tokenize(inner: &str) -> Option<Vec<Tok<'_>>> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut start: Option<usize> = None;
    for (i, ch) in inner.char_indices() {
        if depth > 0 {
            match ch {
                '(' => depth += 1,
                ')' => depth -= 1,
                _ => {}
            }
            continue;
        }
        let sep = match ch {
            ',' => Some(Tok::Comma),
            '/' => Some(Tok::Slash),
            c if c.is_whitespace() => None,
            '(' => {
                depth += 1;
                start.get_or_insert(i);
                continue;
            }
            ')' => return None,
            _ => {
                start.get_or_insert(i);
                continue;
            }
        };
        if let Some(s) = start.take() {
            out.push(Tok::Arg(&inner[s..i]));
        }
        if let Some(sep) = sep {
            out.push(sep);
        }
    }
    if depth != 0 {
        return None;
    }
    if let Some(s) = start {
        out.push(Tok::Arg(&inner[s..]));
    }
    Some(out)
}

fn parse_arg(s: &str) -> Option<Arg> {
    if s == "none" {
        return Some(Arg::None);
    }
    if let Some(expr) = s.strip_prefix("calc(").and_then(|e| e.strip_suffix(')')) {
        return calc::eval(expr);
    }
    dimension(s)
}

/// A number, percentage or angle token.
fn dimension(s: &str) -> Option<Arg> {
    let split = s
        .find(|c: char| c.is_ascii_alphabetic() && c != 'e' || c == '%')
        .unwrap_or(s.len());
    // `1e2` is a number but `1em` is not; a trailing `e` belongs to a unit.
    let (num, unit) = s.split_at(split);
    let (num, unit) = match num.strip_suffix('e') {
        Some(n) if !unit.is_empty() => (n, &s[n.len()..]),
        _ => (num, unit),
    };
    let n = number(num)?;
    Some(match unit {
        "" => Arg::Number(n),
        "%" => Arg::Percent(n),
        "deg" => Arg::Degrees(n),
        "rad" => Arg::Degrees(n.to_degrees()),
        "grad" => Arg::Degrees(n * 9.0 / 10.0),
        "turn" => Arg::Degrees(n * 360.0),
        _ => return None,
    })
}

/// A CSS `<number>`: digits, an optional sign, point and exponent — not
/// Rust's `inf` / `NaN`.
fn number(s: &str) -> Option<f32> {
    if !s.bytes().any(|b| b.is_ascii_digit())
        || !s
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'.' | b'+' | b'-' | b'e' | b'E'))
    {
        return None;
    }
    s.parse::<f32>().ok().filter(|n| n.is_finite())
}

/// `calc()` over numbers, percentages and angles: `+ - * /`, parentheses
/// and nested `calc()`. Sums need one kind; products need a plain number on
/// one side; division needs a number on the right.
mod calc {
    use super::{dimension, Arg};

    pub(super) fn eval(expr: &str) -> Option<Arg> {
        let toks = lex(expr)?;
        let mut pos = 0;
        let v = sum(&toks, &mut pos)?;
        (pos == toks.len()).then_some(v)
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    enum T {
        V(Arg),
        Op(u8),
        Open,
        Close,
    }

    fn lex(s: &str) -> Option<Vec<T>> {
        let b = s.as_bytes();
        let mut out = Vec::new();
        let mut i = 0;
        while i < b.len() {
            let c = b[i];
            let operand_expected = matches!(out.last(), None | Some(T::Op(_)) | Some(T::Open));
            if c.is_ascii_whitespace() {
                i += 1;
            } else if c == b'(' {
                out.push(T::Open);
                i += 1;
            } else if s[i..].starts_with("calc(") {
                out.push(T::Open);
                i += 5;
            } else if c == b')' {
                out.push(T::Close);
                i += 1;
            } else if c.is_ascii_digit()
                || c == b'.'
                || (operand_expected && matches!(c, b'+' | b'-'))
            {
                let start = i;
                i += 1;
                while i < b.len() {
                    let d = b[i];
                    let exp_sign = matches!(d, b'+' | b'-') && matches!(b[i - 1], b'e' | b'E');
                    if d.is_ascii_alphanumeric() || d == b'.' || d == b'%' || exp_sign {
                        i += 1;
                    } else {
                        break;
                    }
                }
                out.push(T::V(dimension(&s[start..i])?));
            } else if matches!(c, b'+' | b'-' | b'*' | b'/') {
                out.push(T::Op(c));
                i += 1;
            } else {
                return None;
            }
        }
        Some(out)
    }

    fn sum(t: &[T], pos: &mut usize) -> Option<Arg> {
        let mut acc = product(t, pos)?;
        while let Some(T::Op(op @ (b'+' | b'-'))) = t.get(*pos).copied() {
            *pos += 1;
            let rhs = product(t, pos)?;
            let sign = if op == b'+' { 1.0 } else { -1.0 };
            acc = match (acc, rhs) {
                (Arg::Number(a), Arg::Number(b)) => Arg::Number(a + sign * b),
                (Arg::Percent(a), Arg::Percent(b)) => Arg::Percent(a + sign * b),
                (Arg::Degrees(a), Arg::Degrees(b)) => Arg::Degrees(a + sign * b),
                _ => return None,
            };
        }
        Some(acc)
    }

    fn product(t: &[T], pos: &mut usize) -> Option<Arg> {
        let mut acc = atom(t, pos)?;
        while let Some(T::Op(op @ (b'*' | b'/'))) = t.get(*pos).copied() {
            *pos += 1;
            let rhs = atom(t, pos)?;
            acc = match (op, acc, rhs) {
                (b'*', a, Arg::Number(k)) | (b'*', Arg::Number(k), a) => scale(a, k),
                (b'/', a, Arg::Number(k)) if k != 0.0 => scale(a, 1.0 / k),
                _ => return None,
            };
        }
        Some(acc)
    }

    fn scale(a: Arg, k: f32) -> Arg {
        match a {
            Arg::Number(n) => Arg::Number(n * k),
            Arg::Percent(n) => Arg::Percent(n * k),
            Arg::Degrees(n) => Arg::Degrees(n * k),
            Arg::None => Arg::None,
        }
    }

    fn atom(t: &[T], pos: &mut usize) -> Option<Arg> {
        match t.get(*pos)? {
            T::V(v) => {
                *pos += 1;
                Some(*v)
            }
            T::Open => {
                *pos += 1;
                let v = sum(t, pos)?;
                if t.get(*pos) != Some(&T::Close) {
                    return None;
                }
                *pos += 1;
                Some(v)
            }
            _ => None,
        }
    }
}
