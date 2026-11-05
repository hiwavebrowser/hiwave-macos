//! Module graph prefetch (S15): fetch a module script's whole static import
//! graph as soon as the script is discovered, as Chromium does, instead of
//! one level at a time when the module's turn to run comes.
//!
//! The page's script fetch (`fetch_page_scripts`) hands every module script
//! it has (its URL and source) to a [`walk`]er. The walker reads the static
//! `import`/`export ... from` specifiers out of the source with a small
//! lexer (no JavaScript engine: the bindings are not there yet), resolves
//! them as the module host would, and fetches every module it reaches,
//! concurrently and without waiting for a level to finish, through the same
//! policy entry point as the module host's own fetch. Each answer (source or
//! error) is kept in a [`ModulePrefetch`] by the URL that was asked for.
//!
//! Boa stays the authority on the graph. When it asks for an import, the
//! pump takes the prefetched answer if there is one and fetches otherwise.
//! A specifier the lexer misses is fetched when Boa asks for it, as before;
//! one it finds that is not really an import costs a request and nothing
//! else (it is never parsed or run). Dynamic `import()` is not followed.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use futures::stream::{FuturesUnordered, StreamExt};
use rustkit_net::policy::FetchPolicy;
use rustkit_net::ResourceLoader;
use url::Url;

use crate::inspector::Inspector;
use crate::script_net::{fetch_module, MAX_MODULES_PER_PAGE};

/// A module's answer as the module host's fetch gives it.
pub(crate) type ModuleOutcome = Result<rustkit_bindings::FetchedModule, String>;

/// Prefetched module answers by requested URL. Taken (removed) on use: a
/// URL Boa asks for is asked for once.
#[derive(Clone, Default)]
pub(crate) struct ModulePrefetch(Rc<RefCell<HashMap<String, ModuleOutcome>>>);

impl ModulePrefetch {
    pub(crate) fn take(&self, url: &str) -> Option<ModuleOutcome> {
        self.0.borrow_mut().remove(url)
    }

    fn put(&self, url: String, outcome: ModuleOutcome) {
        self.0.borrow_mut().entry(url).or_insert(outcome);
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.0.borrow().len()
    }
}

/// A module script the page's script fetch has: the URL it was asked for
/// (`None` for an inline module), the URL its imports resolve against, and
/// its source.
pub(crate) struct DiscoveredModule {
    pub(crate) requested: Option<Url>,
    pub(crate) base: Url,
    pub(crate) source: String,
}

/// Walk the static import graphs of the modules `discovered` yields until
/// it closes and nothing is out. Answers land in `prefetch`. `import_maps`
/// are the document's inline import maps' texts, merged in order.
pub(crate) async fn walk(
    mut discovered: futures::channel::mpsc::UnboundedReceiver<DiscoveredModule>,
    prefetch: ModulePrefetch,
    policy: Arc<FetchPolicy>,
    loader: Arc<ResourceLoader>,
    document: Url,
    import_maps: Vec<String>,
    deadline: tokio::time::Instant,
    inspector: Arc<Inspector>,
) {
    let mut map = rustkit_js::ImportMap::default();
    for text in &import_maps {
        // An unusable map is the run's to report; here it changes nothing.
        if let Ok((parsed, _)) = rustkit_js::ImportMap::parse(text, &document) {
            map.merge(parsed);
        }
    }
    // Every URL fetched or already in hand, so a URL is requested once.
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = FuturesUnordered::new();
    let mut open = true;
    let mut started = 0usize;
    let fetch = |url: String| {
        let (policy, loader, document, inspector) = (policy.clone(), loader.clone(), document.clone(), inspector.clone());
        async move {
            let outcome = fetch_module(&policy, &loader, &url, &document, deadline, &inspector).await;
            (url, outcome)
        }
    };
    let follow = |base: &Url, source: &str, seen: &mut HashSet<String>, started: &mut usize| -> Vec<String> {
        let mut next = Vec::new();
        for specifier in static_import_specifiers(source) {
            // Resolution as the module host's: relative to the importer,
            // the import map for the rest; a bare specifier with no mapping
            // is an error there and nothing to fetch here.
            let Ok(url) = map.resolve(&specifier, base, base) else { continue };
            if !matches!(url.scheme(), "http" | "https") {
                continue;
            }
            let url = url.to_string();
            if *started >= MAX_MODULES_PER_PAGE || !seen.insert(url.clone()) {
                continue;
            }
            *started += 1;
            next.push(url);
        }
        next
    };
    loop {
        if !open && out.is_empty() {
            return;
        }
        tokio::select! {
            root = discovered.next(), if open => match root {
                Some(root) => {
                    if let Some(requested) = &root.requested {
                        // The tag's own fetch answered this URL: an import of
                        // it is answered from here, not fetched again.
                        let requested = requested.to_string();
                        if seen.insert(requested.clone()) {
                            prefetch.put(
                                requested,
                                Ok(rustkit_bindings::FetchedModule {
                                    final_url: root.base.to_string(),
                                    source: root.source.clone(),
                                }),
                            );
                        }
                    }
                    for url in follow(&root.base, &root.source, &mut seen, &mut started) {
                        out.push(fetch(url));
                    }
                }
                None => open = false,
            },
            Some((url, outcome)) = out.next(), if !out.is_empty() => {
                if let Ok(module) = &outcome {
                    if let Ok(base) = Url::parse(&module.final_url) {
                        // A redirect's final URL names the same module.
                        seen.insert(module.final_url.clone());
                        for next in follow(&base, &module.source, &mut seen, &mut started) {
                            out.push(fetch(next));
                        }
                    }
                }
                prefetch.put(url, outcome);
            }
        }
    }
}

/// The module specifiers of `source`'s static imports and re-exports, in
/// source order: `import 'x'`, `import ... from 'x'`, `export ... from 'x'`.
/// Not `import('x')` or `import.meta`. Comments, strings, templates and
/// (by the usual heuristic) regular expression literals are skipped.
pub(crate) fn static_import_specifiers(source: &str) -> Vec<String> {
    let b = source.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    // The last significant byte, for the regex-or-division guess, and
    // whether it ended an identifier/keyword (and which).
    let mut last: u8 = b'\n';
    let mut last_word: &str = "";
    // `${` nesting inside templates: a `}` at this brace depth resumes the
    // template.
    let mut template_braces: Vec<usize> = Vec::new();
    let mut braces = 0usize;
    while i < b.len() {
        let c = b[i];
        match c {
            b' ' | b'\t' | b'\n' | b'\r' => i += 1,
            b'/' if b.get(i + 1) == Some(&b'/') => i = skip_line(b, i),
            b'/' if b.get(i + 1) == Some(&b'*') => i = skip_block(b, i),
            b'/' => {
                if regex_allowed(last, last_word) {
                    i = skip_regex(b, i);
                    last = b'a';
                    last_word = "";
                } else {
                    i += 1;
                    last = c;
                    last_word = "";
                }
            }
            b'\'' | b'"' => {
                i = skip_string(b, i);
                last = b'"';
                last_word = "";
            }
            b'`' => {
                i = skip_template(b, i + 1, &mut template_braces, braces);
                last = b'`';
                last_word = "";
            }
            b'{' => {
                braces += 1;
                i += 1;
                last = c;
                last_word = "";
            }
            b'}' => {
                if template_braces.last() == Some(&braces) {
                    template_braces.pop();
                    braces = braces.saturating_sub(1);
                    i = skip_template(b, i + 1, &mut template_braces, braces);
                    last = b'`';
                } else {
                    braces = braces.saturating_sub(1);
                    i += 1;
                    last = c;
                }
                last_word = "";
            }
            _ if is_ident_start(c) => {
                let start = i;
                while i < b.len() && is_ident_part(b[i]) {
                    i += 1;
                }
                let word = &source[start..i];
                // `x.import` is a property, not the keyword.
                let property = last == b'.';
                if !property && (word == "import" || word == "export") {
                    if let Some((specifier, end)) = clause_specifier(source, i, word == "import") {
                        out.push(specifier);
                        i = end;
                        last = b'"';
                        last_word = "";
                        continue;
                    }
                }
                last = b'a';
                last_word = word;
            }
            _ => {
                i += 1;
                last = c;
                last_word = "";
            }
        }
    }
    out
}

fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_' || c == b'$' || c >= 0x80
}

fn is_ident_part(c: u8) -> bool {
    is_ident_start(c) || c.is_ascii_digit()
}

/// A `/` here starts a regular expression rather than dividing.
fn regex_allowed(last: u8, last_word: &str) -> bool {
    if !last_word.is_empty() {
        return matches!(
            last_word,
            "return" | "typeof" | "instanceof" | "in" | "of" | "new" | "delete" | "void" | "throw" | "case" | "do"
                | "else" | "yield" | "await"
        );
    }
    !matches!(last, b')' | b']' | b'}' | b'"' | b'`' | b'a' | b'0'..=b'9')
}

fn skip_line(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && b[i] != b'\n' {
        i += 1;
    }
    i
}

fn skip_block(b: &[u8], mut i: usize) -> usize {
    i += 2;
    while i < b.len() {
        if b[i] == b'*' && b.get(i + 1) == Some(&b'/') {
            return i + 2;
        }
        i += 1;
    }
    i
}

fn skip_string(b: &[u8], mut i: usize) -> usize {
    let quote = b[i];
    i += 1;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            c if c == quote => return i + 1,
            b'\n' => return i + 1,
            _ => i += 1,
        }
    }
    i
}

fn skip_regex(b: &[u8], mut i: usize) -> usize {
    i += 1;
    let mut class = false;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'[' => {
                class = true;
                i += 1;
            }
            b']' => {
                class = false;
                i += 1;
            }
            b'/' if !class => {
                i += 1;
                while i < b.len() && is_ident_part(b[i]) {
                    i += 1;
                }
                return i;
            }
            b'\n' => return i,
            _ => i += 1,
        }
    }
    i
}

/// From just inside a template (after `` ` `` or a substitution's `}`) to
/// just past its end, or into a `${` (recorded at brace depth `braces + 1`).
fn skip_template(b: &[u8], mut i: usize, template_braces: &mut Vec<usize>, braces: usize) -> usize {
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'`' => return i + 1,
            b'$' if b.get(i + 1) == Some(&b'{') => {
                // The `{` counts as an ordinary brace once we are back in code.
                template_braces.push(braces + 1);
                return i + 1;
            }
            _ => i += 1,
        }
    }
    i
}

/// Skip whitespace and comments.
fn skip_trivia(b: &[u8], mut i: usize) -> usize {
    loop {
        match b.get(i) {
            Some(b' ' | b'\t' | b'\n' | b'\r') => i += 1,
            Some(b'/') if b.get(i + 1) == Some(&b'/') => i = skip_line(b, i),
            Some(b'/') if b.get(i + 1) == Some(&b'*') => i = skip_block(b, i),
            _ => return i,
        }
    }
}

/// A string literal's value at `i` (escapes taken literally but for the
/// common `\'`/`\"`/`\\`), and the index past it.
fn string_at(source: &str, i: usize) -> Option<(String, usize)> {
    let b = source.as_bytes();
    let quote = *b.get(i)?;
    if quote != b'\'' && quote != b'"' {
        return None;
    }
    let end = skip_string(b, i);
    if end < i + 2 || b[end - 1] != quote {
        return None;
    }
    let raw = &source[i + 1..end - 1];
    if raw.contains('\n') {
        return None;
    }
    let value = raw.replace("\\'", "'").replace("\\\"", "\"").replace("\\\\", "\\");
    Some((value, end))
}

/// After the keyword `import`/`export` (ending at `i`): the specifier of a
/// static import or re-export, and where its string ends.
fn clause_specifier(source: &str, i: usize, import: bool) -> Option<(String, usize)> {
    let b = source.as_bytes();
    let mut j = skip_trivia(b, i);
    if import {
        // `import 'x'`
        if let Some(found) = string_at(source, j) {
            return Some(found);
        }
        // `import(` and `import.meta` are not static imports.
        if matches!(b.get(j), Some(b'(' | b'.')) {
            return None;
        }
    } else if !matches!(b.get(j), Some(b'*' | b'{')) {
        // `export const ...`, `export default ...`: no source.
        return None;
    }
    // The clause: names, `*`, `as`, commas and one `{...}`, up to `from`.
    let mut words = 0;
    loop {
        j = skip_trivia(b, j);
        match b.get(j)? {
            b'{' => {
                let close = j + source[j..].find('}')?;
                if source[j..close].contains(['(', ';', '=']) {
                    return None;
                }
                j = close + 1;
            }
            b'*' | b',' => j += 1,
            c if is_ident_start(*c) => {
                let start = j;
                while j < b.len() && is_ident_part(b[j]) {
                    j += 1;
                }
                if &source[start..j] == "from" && (words > 0 || !import) {
                    let k = skip_trivia(b, j);
                    if let Some(found) = string_at(source, k) {
                        return Some(found);
                    }
                }
                words += 1;
                if words > 64 {
                    return None;
                }
            }
            _ => return None,
        }
        // A clause's `{...}`/`*` counts as a word for `import { x } from`.
        words = words.max(1);
    }
}

#[cfg(test)]
mod tests {
    use super::static_import_specifiers as specs;

    #[test]
    fn finds_static_imports_and_re_exports() {
        let src = r#"
import 'side-effect.js';
import def from "./def.js";
import * as ns from './ns.js';
import def2, { a, b as c } from './named.js';
import {
    x,
    y,
} from './multi.js';
export * from './star.js';
export * as all from './star-as.js';
export { q } from './re.js';
export { local };
export const z = 1;
export default function () {}
import json from './data.json' with { type: 'json' };
"#;
        assert_eq!(
            specs(src),
            vec![
                "side-effect.js",
                "./def.js",
                "./ns.js",
                "./named.js",
                "./multi.js",
                "./star.js",
                "./star-as.js",
                "./re.js",
                "./data.json",
            ]
        );
    }

    #[test]
    fn skips_dynamic_imports_meta_comments_strings_templates_and_regexes() {
        let src = r#"
const m = import('./dynamic.js');
const u = import.meta.url;
// import './line-comment.js';
/* import x from './block-comment.js'; */
const s = "import './double.js'";
const t = 'export * from "./single.js"';
const tpl = `import './template.js' ${ `nested ${1}` } import './template2.js'`;
const re = /import '.\/regex.js'/g;
const o = { import: 1 }; o.import;
const ratio = a / b; import './real.js';
"#;
        assert_eq!(specs(src), vec!["./real.js"]);
    }

    #[test]
    fn minified_imports_are_found() {
        assert_eq!(
            specs(r#"import{a as b}from"./a.js";import"./b.js";export*from"./c.js";const x=1/2;"#),
            vec!["./a.js", "./b.js", "./c.js"]
        );
    }
}
