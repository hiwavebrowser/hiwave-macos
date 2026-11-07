//! Parse-time validity of keyword-only and `<number>` properties (CH2.E29b).
//!
//! A declaration whose value is one word a keyword-only property does not
//! take (`object-fit: bogus`, `position: middle`) is invalid: it is not in
//! the declaration block, so the value the cascade already had stands. The
//! property arms used to reset such a property to its default instead, so
//! `position: middle` in a style attribute wiped a sheet's `relative`.
//!
//! The words are Chromium 148's (`tools/parity_oracle/keyword_grammar_log.mjs`).
//! Only a lone identifier is judged: two words, a function or `var()` are
//! left to the property arms as before, so a word missing here can at worst
//! keep an invalid value, never drop a valid one. The bindings' `KEYWORDS`
//! table in `WRAPPERS_JS` is the same list (a bindings test keeps them equal).

const SIDE: &str = "dashed dotted double groove hidden inset none outset ridge solid";
const OVERFLOW: &str = "auto clip hidden overlay scroll visible";
const WRAP: &str = "anywhere break-word normal";

/// `(property, space-separated words)`, every keyword-only property judged.
pub const KEYWORDS: &[(&str, &str)] = &[
    ("object-fit", "contain cover fill none scale-down"),
    ("position", "absolute fixed relative static sticky"),
    ("float", "inline-end inline-start left none right"),
    ("clear", "both inline-end inline-start left none right"),
    ("visibility", "collapse hidden visible"),
    ("box-sizing", "border-box content-box"),
    ("flex-direction", "column column-reverse row row-reverse"),
    ("flex-wrap", "nowrap wrap wrap-reverse"),
    ("table-layout", "auto fixed"),
    ("border-collapse", "collapse separate"),
    ("empty-cells", "hide show"),
    ("caption-side", "bottom top"),
    ("list-style-position", "inside outside"),
    ("direction", "ltr rtl"),
    ("isolation", "auto isolate"),
    ("backface-visibility", "hidden visible"),
    (
        "mix-blend-mode",
        "color color-burn color-dodge darken difference exclusion hard-light hue lighten luminosity \
         multiply normal overlay plus-lighter saturation screen soft-light",
    ),
    ("overflow-x", OVERFLOW),
    ("overflow-y", OVERFLOW),
    ("resize", "auto block both horizontal inline none vertical"),
    ("text-overflow", "clip ellipsis"),
    ("user-select", "all auto none text"),
    ("box-decoration-break", "clone slice"),
    ("scroll-behavior", "auto smooth"),
    ("text-decoration-style", "dashed dotted double solid wavy"),
    ("border-top-style", SIDE),
    ("border-right-style", SIDE),
    ("border-bottom-style", SIDE),
    ("border-left-style", SIDE),
    ("outline-style", "auto dashed dotted double groove inset none outset ridge solid"),
    ("font-kerning", "auto none normal"),
    ("content-visibility", "auto hidden visible"),
    ("hyphens", "auto manual none"),
    (
        "white-space",
        "break-spaces collapse normal nowrap pre pre-line pre-wrap preserve preserve-breaks wrap",
    ),
    (
        "text-align",
        "-webkit-auto -webkit-center -webkit-left -webkit-match-parent -webkit-right center end \
         justify left right start",
    ),
    ("word-break", "auto-phrase break-all break-word keep-all normal"),
    ("overflow-wrap", WRAP),
    ("word-wrap", WRAP),
    ("line-break", "anywhere auto loose normal strict"),
    (
        "writing-mode",
        "horizontal-tb lr lr-tb rl rl-tb sideways-lr sideways-rl tb tb-rl vertical-lr vertical-rl",
    ),
    (
        "unicode-bidi",
        "-webkit-isolate -webkit-isolate-override -webkit-plaintext bidi-override embed isolate \
         isolate-override normal plaintext",
    ),
    ("image-rendering", "-webkit-optimize-contrast auto crisp-edges pixelated"),
    ("background-attachment", "fixed local scroll"),
];

/// Properties whose one value is a non-negative `<number>`.
pub const NUMBERS: &[&str] = &["flex-grow", "flex-shrink"];

const CSS_WIDE: &[&str] = &["initial", "inherit", "unset", "revert", "revert-layer"];

fn is_ident(word: &str) -> bool {
    let w = word.strip_prefix('-').unwrap_or(word);
    let mut chars = w.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase() || c == '_')
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

/// Chromium's non-negative `<number>`: `.5`, `+2`, `1e1`; not `1.`, `-1`, `1px`.
fn is_non_negative_number(word: &str) -> bool {
    let w = word.strip_prefix('+').unwrap_or(word);
    let (mantissa, exponent) = match w.find('e') {
        Some(i) => (&w[..i], Some(&w[i + 1..])),
        None => (w, None),
    };
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let mantissa_ok = match mantissa.split_once('.') {
        Some((int, frac)) => (int.is_empty() || digits(int)) && digits(frac),
        None => digits(mantissa),
    };
    let exponent_ok = match exponent {
        None => true,
        Some(e) => digits(e.strip_prefix(['+', '-']).unwrap_or(e)),
    };
    mantissa_ok && exponent_ok
}

/// Is `value` (as written, before `var()` substitution) invalid for
/// `property` at parse time, so the declaration is dropped?
pub fn invalid_value(property: &str, value: &str) -> bool {
    let word = value.trim().to_ascii_lowercase();
    if CSS_WIDE.contains(&word.as_str()) {
        return false;
    }
    if let Some((_, words)) = KEYWORDS.iter().find(|(p, _)| *p == property) {
        return is_ident(&word) && !words.split_whitespace().any(|w| w == word);
    }
    if NUMBERS.contains(&property) && !word.contains(|c: char| c.is_whitespace() || c == '(') {
        return !is_non_negative_number(&word);
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lone_unknown_word_is_invalid_for_a_keyword_property() {
        assert!(invalid_value("object-fit", "bogus"));
        assert!(invalid_value("position", "default"));
        assert!(invalid_value("position", "-webkit-sticky"));
        assert!(!invalid_value("position", "Relative"));
        assert!(!invalid_value("position", "inherit"));
        assert!(!invalid_value("position", "var(--p)"));
        assert!(!invalid_value("text-overflow", "clip ellipsis"));
        assert!(!invalid_value("display", "bogus"));
    }

    #[test]
    fn flex_factors_take_a_non_negative_number() {
        for ok in ["0", ".5", "+2", "1e1", "1.5E-1", "calc(1)", "unset"] {
            assert!(!invalid_value("flex-grow", ok), "{ok}");
        }
        for bad in ["-1", "1px", "1.", "1e", "abc", "\"s\""] {
            assert!(invalid_value("flex-shrink", bad), "{bad}");
        }
    }
}
