use rustkit_html::tokenizer::{tokenize, Token};

#[test]
fn references_in_text_rcdata_and_attributes() {
    let source = "&amp;&#128;&#x1F600;&unknown;&é;&;&#x;&unfinished";
    let expected = "&€😀&unknown;&é;&;&#x;&unfinished";
    let html = format!(
        "<p a='{source}' b=\"{source}\" c={source}>{source}</p><textarea>{source}</textarea>"
    );
    let tokens = tokenize(&html).unwrap();
    let Token::StartTag { attrs, .. } = &tokens[0] else {
        panic!("expected a start tag");
    };
    for name in ["a", "b", "c"] {
        assert_eq!(attrs.get(name).map(String::as_str), Some(expected));
    }
    let text: String = tokens
        .iter()
        .filter_map(|token| match token {
            Token::Character(c) => Some(*c),
            _ => None,
        })
        .collect();
    assert_eq!(text, expected.repeat(2));
}

#[test]
fn reference_scan_stops_at_32_bytes() {
    // The scanner includes the opening ampersand in its byte count.
    // Padding the same code point makes the boundary visible in the output.
    let at_limit = format!("&#{:030};", 65);
    let over_limit = format!("&#{:031};", 65);
    let html = format!("{at_limit}{over_limit}&amp;");
    assert_text_tokens(&html, &format!("A{over_limit}&"));
}

#[test]
fn reference_at_end_of_input() {
    assert_text_tokens("last&#x1F600;", "last😀");
    assert_text_tokens("last&#x", "last&#x");
    assert_text_tokens("last&", "last&");
}

fn assert_text_tokens(html: &str, text: &str) {
    let expected: Vec<_> = text
        .chars()
        .map(Token::Character)
        .chain([Token::Eof])
        .collect();
    assert_eq!(tokenize(html).unwrap(), expected, "{html}");
}

#[test]
#[ignore = "release-mode tokenizer timing"]
fn entity_tokenizer_timing() {
    for (label, value) in [("plain", "abcdef"), ("entities", "&amp;&#65;&#x41;&lt;")] {
        let html = format!("<p title='{value}'>{value}</p>").repeat(2048);
        let mut samples = Vec::new();
        for _ in 0..31 {
            let start = std::time::Instant::now();
            std::hint::black_box(tokenize(std::hint::black_box(&html)).unwrap());
            samples.push(start.elapsed());
        }
        samples.sort_unstable();
        eprintln!(
            "{label}: median {:?} per document",
            samples[samples.len() / 2]
        );
    }
}
