//! HTML entity decoding.
//!
//! Provides decoding for HTML5 named character references.
//! Covers the most commonly used entities in web content.

use std::collections::HashMap;

lazy_static::lazy_static! {
    /// Named HTML entities lookup table.
    /// Covers commonly used entities plus special characters.
    static ref ENTITIES: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();

        // ==================== BASIC REQUIRED ENTITIES ====================
        m.insert("lt", "<");
        m.insert("gt", ">");
        m.insert("amp", "&");
        m.insert("quot", "\"");
        m.insert("apos", "'");

        // ==================== WHITESPACE & SPECIAL ====================
        m.insert("nbsp", "\u{00A0}");   // Non-breaking space
        m.insert("ensp", "\u{2002}");   // En space
        m.insert("emsp", "\u{2003}");   // Em space
        m.insert("thinsp", "\u{2009}"); // Thin space
        m.insert("zwnj", "\u{200C}");   // Zero-width non-joiner
        m.insert("zwj", "\u{200D}");    // Zero-width joiner
        m.insert("lrm", "\u{200E}");    // Left-to-right mark
        m.insert("rlm", "\u{200F}");    // Right-to-left mark
        m.insert("shy", "\u{00AD}");    // Soft hyphen

        // ==================== PUNCTUATION ====================
        m.insert("ndash", "–");         // En dash
        m.insert("mdash", "—");         // Em dash
        m.insert("lsquo", "\u{2018}");  // Left single quote
        m.insert("rsquo", "\u{2019}");  // Right single quote
        m.insert("sbquo", "\u{201A}");  // Single low-9 quote
        m.insert("ldquo", "\u{201C}");  // Left double quote
        m.insert("rdquo", "\u{201D}");  // Right double quote
        m.insert("bdquo", "\u{201E}");  // Double low-9 quote
        m.insert("lsaquo", "\u{2039}"); // Left single angle quote
        m.insert("rsaquo", "\u{203A}"); // Right single angle quote
        m.insert("laquo", "«");         // Left double angle quote
        m.insert("raquo", "»");         // Right double angle quote
        m.insert("hellip", "…");        // Horizontal ellipsis
        m.insert("bull", "•");          // Bullet
        m.insert("middot", "·");        // Middle dot
        m.insert("prime", "′");         // Prime
        m.insert("Prime", "″");         // Double prime
        m.insert("oline", "‾");         // Overline
        m.insert("frasl", "⁄");         // Fraction slash

        // ==================== SYMBOLS & MARKS ====================
        m.insert("copy", "©");
        m.insert("reg", "®");
        m.insert("trade", "™");
        m.insert("dagger", "†");
        m.insert("Dagger", "‡");
        m.insert("permil", "‰");
        m.insert("loz", "◊");           // Lozenge
        m.insert("spades", "♠");
        m.insert("clubs", "♣");
        m.insert("hearts", "♥");
        m.insert("diams", "♦");
        m.insert("check", "✓");
        m.insert("cross", "✗");
        m.insert("star", "☆");
        m.insert("starf", "★");

        // ==================== CURRENCY ====================
        m.insert("cent", "¢");
        m.insert("pound", "£");
        m.insert("curren", "¤");
        m.insert("yen", "¥");
        m.insert("euro", "€");
        m.insert("fnof", "ƒ");          // Florin

        // ==================== MATHEMATICAL ====================
        m.insert("times", "×");
        m.insert("divide", "÷");
        m.insert("plusmn", "±");
        m.insert("minus", "−");
        m.insert("lowast", "∗");        // Asterisk operator
        m.insert("radic", "√");         // Square root
        m.insert("prop", "∝");          // Proportional to
        m.insert("infin", "∞");
        m.insert("ang", "∠");           // Angle
        m.insert("and", "∧");           // Logical and
        m.insert("or", "∨");            // Logical or
        m.insert("cap", "∩");           // Intersection
        m.insert("cup", "∪");           // Union
        m.insert("int", "∫");           // Integral
        m.insert("there4", "∴");        // Therefore
        m.insert("sim", "∼");           // Tilde operator
        m.insert("cong", "≅");          // Approximately equal
        m.insert("asymp", "≈");         // Almost equal
        m.insert("ne", "≠");            // Not equal
        m.insert("equiv", "≡");         // Identical
        m.insert("le", "≤");            // Less than or equal
        m.insert("ge", "≥");            // Greater than or equal
        m.insert("sub", "⊂");           // Subset
        m.insert("sup", "⊃");           // Superset
        m.insert("nsub", "⊄");          // Not subset
        m.insert("sube", "⊆");          // Subset or equal
        m.insert("supe", "⊇");          // Superset or equal
        m.insert("oplus", "⊕");         // Circled plus
        m.insert("otimes", "⊗");        // Circled times
        m.insert("perp", "⊥");          // Perpendicular
        m.insert("sdot", "⋅");          // Dot operator
        m.insert("sum", "∑");           // Summation
        m.insert("prod", "∏");          // Product
        m.insert("part", "∂");          // Partial differential
        m.insert("nabla", "∇");         // Nabla
        m.insert("exist", "∃");         // There exists
        m.insert("forall", "∀");        // For all
        m.insert("empty", "∅");         // Empty set
        m.insert("isin", "∈");          // Element of
        m.insert("notin", "∉");         // Not element of
        m.insert("ni", "∋");            // Contains as member

        // ==================== FRACTIONS & SUPERSCRIPTS ====================
        m.insert("frac14", "¼");
        m.insert("frac12", "½");
        m.insert("frac34", "¾");
        m.insert("frac13", "⅓");
        m.insert("frac23", "⅔");
        m.insert("frac15", "⅕");
        m.insert("frac25", "⅖");
        m.insert("frac35", "⅗");
        m.insert("frac45", "⅘");
        m.insert("frac16", "⅙");
        m.insert("frac56", "⅚");
        m.insert("frac18", "⅛");
        m.insert("frac38", "⅜");
        m.insert("frac58", "⅝");
        m.insert("frac78", "⅞");
        m.insert("sup1", "¹");
        m.insert("sup2", "²");
        m.insert("sup3", "³");
        m.insert("ordf", "ª");          // Feminine ordinal
        m.insert("ordm", "º");          // Masculine ordinal

        // ==================== LATIN EXTENDED ====================
        // Acute accents
        m.insert("Aacute", "Á");
        m.insert("aacute", "á");
        m.insert("Eacute", "É");
        m.insert("eacute", "é");
        m.insert("Iacute", "Í");
        m.insert("iacute", "í");
        m.insert("Oacute", "Ó");
        m.insert("oacute", "ó");
        m.insert("Uacute", "Ú");
        m.insert("uacute", "ú");
        m.insert("Yacute", "Ý");
        m.insert("yacute", "ý");

        // Grave accents
        m.insert("Agrave", "À");
        m.insert("agrave", "à");
        m.insert("Egrave", "È");
        m.insert("egrave", "è");
        m.insert("Igrave", "Ì");
        m.insert("igrave", "ì");
        m.insert("Ograve", "Ò");
        m.insert("ograve", "ò");
        m.insert("Ugrave", "Ù");
        m.insert("ugrave", "ù");

        // Circumflex accents
        m.insert("Acirc", "Â");
        m.insert("acirc", "â");
        m.insert("Ecirc", "Ê");
        m.insert("ecirc", "ê");
        m.insert("Icirc", "Î");
        m.insert("icirc", "î");
        m.insert("Ocirc", "Ô");
        m.insert("ocirc", "ô");
        m.insert("Ucirc", "Û");
        m.insert("ucirc", "û");

        // Tilde
        m.insert("Atilde", "Ã");
        m.insert("atilde", "ã");
        m.insert("Ntilde", "Ñ");
        m.insert("ntilde", "ñ");
        m.insert("Otilde", "Õ");
        m.insert("otilde", "õ");

        // Umlaut (diaeresis)
        m.insert("Auml", "Ä");
        m.insert("auml", "ä");
        m.insert("Euml", "Ë");
        m.insert("euml", "ë");
        m.insert("Iuml", "Ï");
        m.insert("iuml", "ï");
        m.insert("Ouml", "Ö");
        m.insert("ouml", "ö");
        m.insert("Uuml", "Ü");
        m.insert("uuml", "ü");
        m.insert("yuml", "ÿ");
        m.insert("Yuml", "Ÿ");

        // Ring
        m.insert("Aring", "Å");
        m.insert("aring", "å");

        // Ligatures and special
        m.insert("AElig", "Æ");
        m.insert("aelig", "æ");
        m.insert("OElig", "Œ");
        m.insert("oelig", "œ");
        m.insert("Ccedil", "Ç");
        m.insert("ccedil", "ç");
        m.insert("Oslash", "Ø");
        m.insert("oslash", "ø");
        m.insert("szlig", "ß");
        m.insert("ETH", "Ð");
        m.insert("eth", "ð");
        m.insert("THORN", "Þ");
        m.insert("thorn", "þ");
        m.insert("Scaron", "Š");
        m.insert("scaron", "š");

        // ==================== GREEK ALPHABET ====================
        m.insert("Alpha", "Α");
        m.insert("alpha", "α");
        m.insert("Beta", "Β");
        m.insert("beta", "β");
        m.insert("Gamma", "Γ");
        m.insert("gamma", "γ");
        m.insert("Delta", "Δ");
        m.insert("delta", "δ");
        m.insert("Epsilon", "Ε");
        m.insert("epsilon", "ε");
        m.insert("Zeta", "Ζ");
        m.insert("zeta", "ζ");
        m.insert("Eta", "Η");
        m.insert("eta", "η");
        m.insert("Theta", "Θ");
        m.insert("theta", "θ");
        m.insert("thetasym", "ϑ");
        m.insert("Iota", "Ι");
        m.insert("iota", "ι");
        m.insert("Kappa", "Κ");
        m.insert("kappa", "κ");
        m.insert("Lambda", "Λ");
        m.insert("lambda", "λ");
        m.insert("Mu", "Μ");
        m.insert("mu", "μ");
        m.insert("Nu", "Ν");
        m.insert("nu", "ν");
        m.insert("Xi", "Ξ");
        m.insert("xi", "ξ");
        m.insert("Omicron", "Ο");
        m.insert("omicron", "ο");
        m.insert("Pi", "Π");
        m.insert("pi", "π");
        m.insert("piv", "ϖ");           // Pi symbol
        m.insert("Rho", "Ρ");
        m.insert("rho", "ρ");
        m.insert("Sigma", "Σ");
        m.insert("sigma", "σ");
        m.insert("sigmaf", "ς");        // Final sigma
        m.insert("Tau", "Τ");
        m.insert("tau", "τ");
        m.insert("Upsilon", "Υ");
        m.insert("upsilon", "υ");
        m.insert("upsih", "ϒ");         // Upsilon with hook
        m.insert("Phi", "Φ");
        m.insert("phi", "φ");
        m.insert("Chi", "Χ");
        m.insert("chi", "χ");
        m.insert("Psi", "Ψ");
        m.insert("psi", "ψ");
        m.insert("Omega", "Ω");
        m.insert("omega", "ω");

        // ==================== ARROWS ====================
        m.insert("larr", "←");          // Left arrow
        m.insert("uarr", "↑");          // Up arrow
        m.insert("rarr", "→");          // Right arrow
        m.insert("darr", "↓");          // Down arrow
        m.insert("harr", "↔");          // Left-right arrow
        m.insert("crarr", "↵");         // Carriage return arrow
        m.insert("lArr", "⇐");          // Left double arrow
        m.insert("uArr", "⇑");          // Up double arrow
        m.insert("rArr", "⇒");          // Right double arrow
        m.insert("dArr", "⇓");          // Down double arrow
        m.insert("hArr", "⇔");          // Left-right double arrow

        // ==================== MISCELLANEOUS SYMBOLS ====================
        m.insert("deg", "°");
        m.insert("micro", "µ");
        m.insert("para", "¶");
        m.insert("sect", "§");
        m.insert("brvbar", "¦");
        m.insert("not", "¬");
        m.insert("macr", "¯");
        m.insert("acute", "´");
        m.insert("cedil", "¸");
        m.insert("uml", "¨");
        m.insert("circ", "ˆ");          // Modifier circumflex
        m.insert("tilde", "˜");         // Small tilde
        m.insert("iexcl", "¡");         // Inverted exclamation
        m.insert("iquest", "¿");        // Inverted question
        m.insert("alefsym", "ℵ");       // Alef symbol
        m.insert("weierp", "℘");        // Weierstrass p
        m.insert("image", "ℑ");         // Imaginary part
        m.insert("real", "ℜ");          // Real part

        m
    };
}

/// Decode HTML entities in a string.
pub fn decode(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '&' {
            // Try to parse an entity
            let mut entity = String::new();
            let mut found_end = false;

            // Look ahead for entity content
            while let Some(&next_ch) = chars.peek() {
                if next_ch == ';' {
                    chars.next(); // consume ';'
                    found_end = true;
                    break;
                } else if next_ch.is_alphanumeric() || next_ch == '#' {
                    entity.push(next_ch);
                    chars.next();
                } else {
                    // Invalid entity character
                    break;
                }

                // Limit entity length to prevent DoS
                if entity.len() > 32 {
                    break;
                }
            }

            if found_end && !entity.is_empty() {
                let mut numeric = [0; 4];
                if let Some(decoded) = decode_reference(&entity, &mut numeric) {
                    result.push_str(decoded);
                    continue;
                }
            }

            // If we couldn't decode, output the original
            result.push('&');
            result.push_str(&entity);
            if found_end {
                result.push(';');
            }
        } else {
            result.push(ch);
        }
    }

    result
}

/// Resolve one scanned, semicolon-terminated name. Named values borrow the
/// table and numeric values use the caller's UTF-8 buffer.
pub(super) fn decode_reference<'a>(name: &str, numeric: &'a mut [u8; 4]) -> Option<&'a str> {
    if let Some(digits) = name.strip_prefix('#') {
        return Some(decode_numeric(digits)?.encode_utf8(numeric));
    }
    ENTITIES.get(name).copied()
}

fn decode_numeric(num_str: &str) -> Option<char> {
    let (radix, digits) = if num_str.starts_with('x') || num_str.starts_with('X') {
        (16, &num_str[1..])
    } else {
        (10, num_str)
    };

    let code_point = u32::from_str_radix(digits, radix).ok()?;

    // Handle replacement characters per HTML5 spec
    let replacement = match code_point {
        // Null character
        0x00 => Some('\u{FFFD}'),
        // Surrogate range (invalid in UTF-8)
        0xD800..=0xDFFF => Some('\u{FFFD}'),
        // Non-characters
        0xFFFE | 0xFFFF => Some('\u{FFFD}'),
        // Beyond Unicode range
        n if n > 0x10FFFF => Some('\u{FFFD}'),
        // C0 control characters (except whitespace)
        0x01..=0x08 | 0x0B | 0x0E..=0x1F | 0x7F => None, // Just map to char
        // C1 control characters - Windows-1252 mappings
        0x80 => Some('€'),
        0x82 => Some('‚'),
        0x83 => Some('ƒ'),
        0x84 => Some('„'),
        0x85 => Some('…'),
        0x86 => Some('†'),
        0x87 => Some('‡'),
        0x88 => Some('ˆ'),
        0x89 => Some('‰'),
        0x8A => Some('Š'),
        0x8B => Some('‹'),
        0x8C => Some('Œ'),
        0x8E => Some('Ž'),
        0x91 => Some('\u{2018}'), // '
        0x92 => Some('\u{2019}'), // '
        0x93 => Some('\u{201C}'), // "
        0x94 => Some('\u{201D}'), // "
        0x95 => Some('•'),
        0x96 => Some('–'),
        0x97 => Some('—'),
        0x98 => Some('˜'),
        0x99 => Some('™'),
        0x9A => Some('š'),
        0x9B => Some('›'),
        0x9C => Some('œ'),
        0x9E => Some('ž'),
        0x9F => Some('Ÿ'),
        _ => None,
    };

    replacement.or_else(|| char::from_u32(code_point))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_basic_entities() {
        assert_eq!(decode("&lt;"), "<");
        assert_eq!(decode("&gt;"), ">");
        assert_eq!(decode("&amp;"), "&");
        assert_eq!(decode("&quot;"), "\"");
        assert_eq!(decode("&apos;"), "'");
    }

    #[test]
    fn test_decode_numeric_entities() {
        assert_eq!(decode("&#65;"), "A");
        assert_eq!(decode("&#x41;"), "A");
        assert_eq!(decode("&#x1F600;"), "😀");
    }

    #[test]
    fn test_decode_multiple_entities() {
        assert_eq!(decode("&lt;div&gt;"), "<div>");
        assert_eq!(decode("Tom &amp; Jerry"), "Tom & Jerry");
    }

    #[test]
    fn test_decode_unknown_entity() {
        assert_eq!(decode("&unknown;"), "&unknown;");
    }

    #[test]
    fn test_decode_no_entities() {
        assert_eq!(decode("hello world"), "hello world");
    }

    #[test]
    fn test_decode_incomplete_entity() {
        assert_eq!(decode("&lt"), "&lt");
        assert_eq!(decode("&"), "&");
    }

    #[test]
    fn test_decode_nbsp() {
        assert_eq!(decode("&nbsp;"), "\u{00A0}");
    }

    #[test]
    fn test_decode_copyright() {
        assert_eq!(decode("&copy;"), "©");
    }

    #[test]
    fn test_decode_greek() {
        assert_eq!(decode("&alpha;"), "α");
        assert_eq!(decode("&Beta;"), "Β");
        assert_eq!(decode("&pi;"), "π");
        assert_eq!(decode("&Omega;"), "Ω");
    }

    #[test]
    fn test_decode_arrows() {
        assert_eq!(decode("&larr;"), "←");
        assert_eq!(decode("&rarr;"), "→");
        assert_eq!(decode("&harr;"), "↔");
    }

    #[test]
    fn test_decode_math() {
        assert_eq!(decode("&times;"), "×");
        assert_eq!(decode("&divide;"), "÷");
        assert_eq!(decode("&infin;"), "∞");
        assert_eq!(decode("&ne;"), "≠");
    }

    #[test]
    fn test_decode_accented() {
        assert_eq!(decode("&eacute;"), "é");
        assert_eq!(decode("&Ntilde;"), "Ñ");
        assert_eq!(decode("&uuml;"), "ü");
        assert_eq!(decode("&ccedil;"), "ç");
    }

    #[test]
    fn test_decode_quotes() {
        assert_eq!(decode("&ldquo;"), "\u{201C}");  // "
        assert_eq!(decode("&rdquo;"), "\u{201D}");  // "
        assert_eq!(decode("&lsquo;"), "\u{2018}");  // '
        assert_eq!(decode("&rsquo;"), "\u{2019}");  // '
    }

    #[test]
    fn test_windows_1252_mapping() {
        // €
        assert_eq!(decode("&#128;"), "€");
        // Smart quotes
        assert_eq!(decode("&#147;"), "\u{201C}");  // "
        assert_eq!(decode("&#148;"), "\u{201D}");  // "
        // Em dash
        assert_eq!(decode("&#151;"), "—");
    }

    #[test]
    fn test_null_replacement() {
        assert_eq!(decode("&#0;"), "\u{FFFD}");
    }

    #[test]
    fn test_surrogate_replacement() {
        // Surrogate pairs should be replaced
        assert_eq!(decode("&#xD800;"), "\u{FFFD}");
        assert_eq!(decode("&#xDFFF;"), "\u{FFFD}");
    }
}
