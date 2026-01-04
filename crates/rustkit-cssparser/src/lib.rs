//! # RustKit CSS Parser
//!
//! This crate provides a RustKit-owned CSS parsing layer, intended to replace the external
//! `cssparser` dependency over time.
//!
//! Current implementation is a **minimal** stylesheet parser suitable for RustKit's current
//! needs: parse basic rules `selector { prop: value; }` into an AST.

use thiserror::Error;

/// Errors that can occur while parsing CSS.
#[derive(Error, Debug, Clone)]
pub enum ParseError {
    #[error("Unexpected end of input")]
    UnexpectedEof,

    #[error("Parse error: {0}")]
    ParseError(String),
}

/// A parsed stylesheet AST.
#[derive(Debug, Default, Clone)]
pub struct StylesheetAst {
    pub rules: Vec<RuleAst>,
}

/// A parsed rule AST.
#[derive(Debug, Clone)]
pub struct RuleAst {
    pub selector: String,
    pub declarations: Vec<DeclarationAst>,
}

/// A parsed declaration AST.
#[derive(Debug, Clone)]
pub struct DeclarationAst {
    pub property: String,
    pub value: String,
    pub important: bool,
}

/// Parse a stylesheet into an AST.
///
/// Notes:
/// - This is not a full CSS parser.
/// - It does not currently support nested rules (`@media`, `@supports`) or complex tokenization.
/// - It attempts to be robust for common author CSS and RustKit test inputs.
pub fn parse_stylesheet(css: &str) -> Result<StylesheetAst, ParseError> {
    let mut out = StylesheetAst::default();

    let mut current_selector = String::new();
    let mut current_property = String::new();
    let mut current_value = String::new();
    let mut current_decls: Vec<DeclarationAst> = Vec::new();

    let mut in_block = false;
    let mut in_value = false;

    let mut chars = css.chars().peekable();
    while let Some(c) = chars.next() {
        // Very small comment skipper: /* ... */
        if c == '/' && chars.peek() == Some(&'*') {
            // consume '*'
            chars.next();
            // consume until */
            while let Some(cc) = chars.next() {
                if cc == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    break;
                }
            }
            continue;
        }

        if !in_block {
            if c == '{' {
                in_block = true;
                current_selector = current_selector.trim().to_string();
                current_property.clear();
                current_value.clear();
                current_decls.clear();
                in_value = false;
            } else {
                current_selector.push(c);
            }
            continue;
        }

        // In block
        if c == '}' {
            flush_decl(&mut current_property, &mut current_value, &mut current_decls);
            let selector = current_selector.trim().to_string();
            if !selector.is_empty() && !current_decls.is_empty() {
                out.rules.push(RuleAst {
                    selector,
                    declarations: current_decls.clone(),
                });
            }

            // reset for next rule
            in_block = false;
            current_selector.clear();
            current_property.clear();
            current_value.clear();
            current_decls.clear();
            in_value = false;
            continue;
        }

        if !in_value {
            if c == ':' {
                in_value = true;
            } else {
                current_property.push(c);
            }
            continue;
        }

        // In value
        if c == ';' {
            flush_decl(&mut current_property, &mut current_value, &mut current_decls);
            in_value = false;
            continue;
        }

        current_value.push(c);
    }

    if in_block {
        // Unclosed block.
        return Err(ParseError::UnexpectedEof);
    }

    Ok(out)
}

fn flush_decl(
    current_property: &mut String,
    current_value: &mut String,
    decls: &mut Vec<DeclarationAst>,
) {
    let property = current_property.trim();
    let value_raw = current_value.trim();
    if property.is_empty() || value_raw.is_empty() {
        current_property.clear();
        current_value.clear();
        return;
    }

    let (value, important) = strip_important(value_raw);
    decls.push(DeclarationAst {
        property: property.to_string(),
        value: value.to_string(),
        important,
    });

    current_property.clear();
    current_value.clear();
}

fn strip_important(value: &str) -> (&str, bool) {
    let lower = value.to_ascii_lowercase();
    if let Some(idx) = lower.rfind("!important") {
        let before = value[..idx].trim_end();
        (before, true)
    } else {
        (value, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_stylesheet() {
        let css = r#"
            body { color: black; }
            .container { width: 100%; height: 10px !important; }
        "#;
        let ast = parse_stylesheet(css).unwrap();
        assert_eq!(ast.rules.len(), 2);
        assert_eq!(ast.rules[0].selector, "body");
        assert_eq!(ast.rules[0].declarations.len(), 1);
        assert_eq!(ast.rules[1].selector, ".container");
        assert_eq!(ast.rules[1].declarations.len(), 2);
        assert!(ast.rules[1].declarations[1].important);
    }

    #[test]
    fn parse_with_comments() {
        let css = r#"
            /* comment */
            body { color: black; /* inside */ width: 10px; }
        "#;
        let ast = parse_stylesheet(css).unwrap();
        assert_eq!(ast.rules.len(), 1);
        assert_eq!(ast.rules[0].declarations.len(), 2);
    }

    #[test]
    fn unclosed_block_is_error() {
        let css = "body { color: black;";
        let err = parse_stylesheet(css).unwrap_err();
        matches!(err, ParseError::UnexpectedEof);
    }
}


