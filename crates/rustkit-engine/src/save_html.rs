//! The settled page as one replayable HTML file (dev-tools T16,
//! `parity-capture --save-html`).
//!
//! Reducing a live page needs the page as it stood when the capture ended:
//! the DOM after its scripts, and the sheets that styled it. This writes
//! that DOM back out with every `<link rel=stylesheet>` replaced, in place,
//! by a `<style>` holding the text the view fetched for it, and the
//! document's adopted sheets after them. Loaded again with `--html-file`
//! (no scripts), the file lays out as the settled page did.
//!
//! What the file is not: scripts are kept as empty, inert elements (their
//! place and attributes are structure; their text is the site's code and
//! would run again), `<noscript>` is emptied when the capture ran script (it
//! was text then), and no cookies, headers or response bodies other than
//! the sheets' are written. The `<base>` it adds is the page URL without
//! its query and fragment. Not carried: shadow roots, form state set from
//! script (`value`, `checked` properties), canvas pixels, frames' documents,
//! and rules a script added through the CSSOM to a sheet that has no text.

use super::*;
use std::sync::Mutex;

/// External sheet texts by URL, as [`Engine::keep_sheet_texts`] keeps them.
pub(crate) type SheetTexts = Arc<Mutex<HashMap<String, String>>>;

/// HTML void elements: no end tag, no children.
const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "basefont", "bgsound", "br", "col", "embed", "frame", "hr", "img", "input", "keygen", "link",
    "meta", "param", "source", "track", "wbr",
];

/// Elements whose Text children serialize unescaped.
const RAW_TEXT_ELEMENTS: &[&str] = &["style", "script", "xmp", "iframe", "noembed", "noframes", "plaintext"];

/// The `type` a saved script gets: not a script type, so nothing runs it.
pub const SAVED_SCRIPT_TYPE: &str = "text/x-hiwave-saved-script";

impl Engine {
    /// Keep the text of every external sheet this view fetches from now on,
    /// for [`Engine::settled_html`]. Call before the load.
    pub fn keep_sheet_texts(&mut self, id: EngineViewId) -> Result<(), EngineError> {
        let view = self.views.get_mut(&id).ok_or(EngineError::ViewNotFound(id))?;
        view.sheet_texts.get_or_insert_with(SheetTexts::default);
        Ok(())
    }

    /// The view's document as it stands, as one HTML file that replays
    /// without the network or script (see the module doc). Sheets come
    /// from [`Engine::keep_sheet_texts`]; a link whose sheet was not kept
    /// (never fetched, failed, or keeping was off) becomes an empty
    /// `<style>` marked `data-saved-sheet="missing"`.
    pub fn settled_html(&self, id: EngineViewId) -> Result<String, EngineError> {
        let view = self.views.get(&id).ok_or(EngineError::ViewNotFound(id))?;
        let document = view
            .document
            .as_ref()
            .ok_or_else(|| EngineError::ViewError("view has no document".into()))?;
        let texts = view
            .sheet_texts
            .as_ref()
            .map(|t| t.lock().unwrap_or_else(|e| e.into_inner()).clone())
            .unwrap_or_default();
        let mut saver = Saver {
            base: view.url.as_ref(),
            texts: &texts,
            script_on: self.config.javascript_enabled,
            base_written: false,
            adopted: document.root().adopted_styles(),
            out: String::new(),
        };
        saver.node(document.root());
        Ok(saver.out)
    }
}

struct Saver<'a> {
    base: Option<&'a Url>,
    texts: &'a HashMap<String, String>,
    script_on: bool,
    base_written: bool,
    adopted: Vec<String>,
    out: String,
}

impl Saver<'_> {
    fn node(&mut self, node: &Rc<Node>) {
        match &node.node_type {
            NodeType::Element { tag_name, attributes, .. } => self.element(node, tag_name, attributes),
            NodeType::Text(data) => {
                let raw = node.parent().and_then(|p| p.tag_name().map(|t| RAW_TEXT_ELEMENTS.contains(&t))).unwrap_or(false);
                if raw {
                    self.out.push_str(data);
                } else {
                    escape_into(data, false, &mut self.out);
                }
            }
            NodeType::Comment(data) => {
                self.out.push_str("<!--");
                self.out.push_str(data);
                self.out.push_str("-->");
            }
            NodeType::ProcessingInstruction { target, data } => {
                self.out.push_str("<?");
                self.out.push_str(target);
                self.out.push(' ');
                self.out.push_str(data);
                self.out.push('>');
            }
            NodeType::DocumentType { name, .. } => {
                self.out.push_str("<!DOCTYPE ");
                self.out.push_str(name);
                self.out.push_str(">\n");
            }
            NodeType::Document | NodeType::DocumentFragment => self.children(node),
        }
    }

    fn children(&mut self, node: &Rc<Node>) {
        for child in node.children() {
            self.node(&child);
        }
    }

    fn element(&mut self, node: &Rc<Node>, tag: &str, attributes: &HashMap<String, String>) {
        let lower = tag.to_ascii_lowercase();
        let rel_stylesheet = lower == "link"
            && attributes.get("rel").is_some_and(|r| r.split_ascii_whitespace().any(|t| t.eq_ignore_ascii_case("stylesheet")));
        if rel_stylesheet {
            return self.link_sheet(attributes);
        }
        if lower == "script" {
            // Kept for its place (`script + div`, `:nth-child`) and its
            // id/class; the rest of it is the site's code.
            self.out.push_str("<script");
            for name in ["id", "class"] {
                if let Some(value) = attributes.get(name) {
                    self.attribute(name, value);
                }
            }
            self.attribute("type", SAVED_SCRIPT_TYPE);
            self.out.push_str("></script>");
            return;
        }
        self.out.push('<');
        self.out.push_str(tag);
        let mut names: Vec<&String> = attributes.keys().collect();
        names.sort();
        for name in names {
            self.attribute(name, &attributes[name]);
        }
        self.out.push('>');
        if lower == "head" {
            self.write_base();
        }
        if VOID_ELEMENTS.contains(&lower.as_str()) {
            return;
        }
        // With script on, `<noscript>` held text, not elements.
        if !(lower == "noscript" && self.script_on) {
            self.children(node);
        }
        if lower == "body" {
            self.write_adopted();
        }
        self.out.push_str("</");
        self.out.push_str(tag);
        self.out.push('>');
    }

    /// The page URL without query or fragment, first in `<head>`, so the
    /// file's images and fonts resolve as the page's did.
    fn write_base(&mut self) {
        if self.base_written {
            return;
        }
        self.base_written = true;
        let Some(base) = self.base.filter(|u| matches!(u.scheme(), "http" | "https")) else { return };
        let mut base = base.clone();
        base.set_query(None);
        base.set_fragment(None);
        let _ = base.set_username("");
        let _ = base.set_password(None);
        self.out.push_str("<base");
        self.attribute("href", base.as_str());
        self.out.push('>');
    }

    fn link_sheet(&mut self, attributes: &HashMap<String, String>) {
        let url = attributes.get("href").and_then(|href| match self.base {
            Some(base) => base.join(href).ok(),
            None => Url::parse(href).ok(),
        });
        let text = url.and_then(|u| self.texts.get(u.as_str()));
        self.out.push_str("<style");
        for name in ["id", "class", "media", "title"] {
            if let Some(value) = attributes.get(name) {
                self.attribute(name, value);
            }
        }
        self.attribute("data-saved-sheet", if text.is_some() { "link" } else { "missing" });
        self.out.push('>');
        if let Some(text) = text {
            push_style_text(text, &mut self.out);
        }
        self.out.push_str("</style>");
    }

    /// `document.adoptedStyleSheets` cascade after the document's sheets:
    /// last in `<body>` they still do.
    fn write_adopted(&mut self) {
        for text in std::mem::take(&mut self.adopted) {
            if text.is_empty() {
                continue;
            }
            self.out.push_str("<style data-saved-sheet=\"adopted\">");
            push_style_text(&text, &mut self.out);
            self.out.push_str("</style>");
        }
    }

    fn attribute(&mut self, name: &str, value: &str) {
        self.out.push(' ');
        self.out.push_str(name);
        self.out.push_str("=\"");
        escape_into(value, true, &mut self.out);
        self.out.push('"');
    }
}

/// Sheet text inside `<style>`: raw text, so only an end tag can break it
/// (`</style` in a string or comment); that one is split.
fn push_style_text(text: &str, out: &mut String) {
    let mut rest = text;
    while let Some(at) = rest.to_ascii_lowercase().find("</style") {
        out.push_str(&rest[..at]);
        out.push_str("<\\/style");
        rest = &rest[at + "</style".len()..];
    }
    out.push_str(rest);
}

/// HTML §13.3 "escaping a string".
fn escape_into(s: &str, attribute: bool, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '\u{a0}' => out.push_str("&nbsp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if attribute => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
}
