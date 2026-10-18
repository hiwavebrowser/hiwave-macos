//! Lit paints through constructable stylesheets and template markers.
//!
//! The fixture under `fixtures/lit/` is the published browser build of
//! lit 3.3.3, lit-element 4.2.2, lit-html 3.3.3 and @lit/reactive-element
//! 2.1.2, each with its LICENSE (BSD-3-Clause). Expected numbers are what
//! that Lit does in Chrome when `adoptedStyleSheets` and
//! `CSSStyleSheet.replace` exist: one `css` result is one adopted sheet
//! and no `<style>` is injected, the template's elements are the shadow
//! children, named slots take the light-DOM children, and `:host` sets
//! `display` and the border box.

use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;

fn lit_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/lit")
}

fn safe_file(url_path: &str) -> Option<PathBuf> {
    let rest = url_path.strip_prefix("/vendor/")?;
    let (pkg, rel) = rest.split_once('/')?;
    let dir = match pkg {
        "lit" | "lit-html" | "lit-element" | "reactive-element" => pkg,
        _ => return None,
    };
    let mut path = lit_root().join(dir);
    for part in rel.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return None;
        }
        path.push(part);
    }
    path.is_file().then_some(path)
}

struct Server {
    port: u16,
}

fn serve(page: String) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let page = page.clone();
            std::thread::spawn(move || {
                let mut request = Vec::new();
                let mut buf = [0u8; 2048];
                let header_end = loop {
                    if let Some(i) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                        break i + 4;
                    }
                    match stream.read(&mut buf) {
                        Ok(0) | Err(_) => return,
                        Ok(n) => request.extend_from_slice(&buf[..n]),
                    }
                };
                let head = String::from_utf8_lossy(&request[..header_end]).to_string();
                let mut path = head.split_whitespace().nth(1).unwrap_or("/").to_string();
                if let Some(q) = path.find('?') {
                    path.truncate(q);
                }
                let (status, content_type, body) = if path == "/" {
                    ("200 OK", "text/html", page.into_bytes())
                } else if let Some(file) = safe_file(&path) {
                    match std::fs::read(&file) {
                        Ok(bytes) => ("200 OK", "text/javascript", bytes),
                        Err(_) => ("404 Not Found", "text/plain", b"missing".to_vec()),
                    }
                } else {
                    ("404 Not Found", "text/plain", b"missing".to_vec())
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(&body);
            });
        }
    });
    Server { port }
}

fn js(engine: &mut Engine, view: EngineViewId, script: &str) -> String {
    let value = engine.execute_script(view, script).unwrap_or_else(|e| panic!("{script}: {e}"));
    value
        .strip_prefix("String(\"")
        .and_then(|v| v.strip_suffix("\")"))
        .unwrap_or(&value)
        .replace("\\\"", "\"")
        .to_string()
}

fn load_page(html: &str) -> (Engine, EngineViewId) {
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 400, height: 300 })
        .expect("view");
    engine.load_html(view, html).expect("load_html");
    (engine, view)
}

const LIT_PAGE: &str = r#"<!DOCTYPE html><html><head>
<script type="importmap">{
  "imports": {
    "lit": "/vendor/lit/index.js",
    "lit/": "/vendor/lit/",
    "lit-html": "/vendor/lit-html/lit-html.js",
    "lit-html/": "/vendor/lit-html/",
    "lit-element": "/vendor/lit-element/lit-element.js",
    "lit-element/": "/vendor/lit-element/",
    "@lit/reactive-element": "/vendor/reactive-element/reactive-element.js",
    "@lit/reactive-element/": "/vendor/reactive-element/"
  }
}</script>
</head><body style="margin:0">
<lit-post id="p" heading="Hello">
  <span id="credit" slot="credit-bar"></span>
  <span id="header" slot="ssr-post-content-header"></span>
  <span id="title-light" slot="title"></span>
  <span id="flair" slot="post-flair"></span>
  <span id="media" slot="post-media-container"></span>
</lit-post>
<script type="module">
import { LitElement, html, css, supportsAdoptingStyleSheets } from 'lit';
class LitPost extends LitElement {
  static properties = { heading: { type: String } };
  static styles = css`:host { display: flex; width: 240px; height: 160px; margin: 0; padding: 0; border: 0; }`;
  render() {
    return html`<div id="heading">${this.heading}</div><slot name="credit-bar"></slot><slot name="ssr-post-content-header"></slot><slot name="title"></slot><slot name="post-flair"></slot><slot name="post-media-container"></slot>`;
  }
}
customElements.define('lit-post', LitPost);
const el = document.getElementById('p');
try {
  await el.updateComplete;
  const root = el.shadowRoot;
  const names = ['credit-bar', 'ssr-post-content-header', 'title', 'post-flair', 'post-media-container'];
  const slots = names.map(function (name) {
    const s = root.querySelector('slot[name="' + name + '"]');
    if (!s) return name + ':missing';
    const a = s.assignedElements();
    return name + ':' + (a[0] ? a[0].id : 'empty');
  }).join(',');
  const heading = root.querySelector('#heading');
  window.__lit = [
    root.adoptedStyleSheets.length,
    root.querySelectorAll('*').length,
    root.querySelectorAll('style').length,
    slots,
    heading ? heading.textContent : 'no-heading',
    supportsAdoptingStyleSheets
  ].join('|');
} catch (e) {
  window.__lit = 'ERR|' + (e && e.name) + '|' + (e && e.message);
}
</script>
</body></html>"#;

/// Chrome, with Lit's adopting-stylesheets path: one adopted sheet, six
/// shadow elements (the heading div and five named slots, no injected
/// `<style>`), each light child assigned, `display: flex`, border box
/// 240×160 from `:host`.
#[test]
fn a_lit_element_adopts_its_styles_and_assigns_named_slots() {
    let server = serve(LIT_PAGE.to_string());
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 400, height: 300 })
        .expect("view");
    let url = Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    if let Err(e) = rt.block_on(engine.load_url(view, url)) {
        let records = engine.script_log(view);
        panic!("load_url: {e}\n{records:#?}");
    }
    let shape = js(&mut engine, view, "window.__lit || 'missing'");
    assert_eq!(
        shape,
        "1|6|0|credit-bar:credit,ssr-post-content-header:header,title:title-light,post-flair:flair,post-media-container:media|Hello|true",
        "script log: {:#?}",
        engine.script_log(view)
    );
    let box_ = js(
        &mut engine,
        view,
        "var el = document.getElementById('p'); var r = el.getBoundingClientRect(); [getComputedStyle(el).display, Math.round(r.width), Math.round(r.height)].join('|')",
    );
    assert_eq!(box_, "flex|240|160");
}

const HOST_PAGE: &str = r#"<!DOCTYPE html><html><head></head>
<body style="margin:0">
<x-box id="h"><span id="t" slot="title">T</span></x-box>
<p id="p">P</p>
</body></html>"#;

/// The same cascade without Lit: `new CSSStyleSheet` + `replaceSync` +
/// `adoptedStyleSheets` on a shadow root and on the document, `::slotted`
/// and `:host`, a later `replaceSync` republished, and Lit's `<?lit$…$>`
/// marker kept as a comment whose data starts with `?`.
#[test]
fn adopted_stylesheets_style_the_shadow_root_and_the_document() {
    let (mut engine, view) = load_page(HOST_PAGE);
    engine
        .execute_script(
            view,
            r#"
class XBox extends HTMLElement {
  constructor() {
    super();
    var root = this.attachShadow({ mode: 'open' });
    this._sheet = new CSSStyleSheet();
    this._sheet.replaceSync(':host { display: flex; width: 180px; height: 64px; margin: 0; padding: 0; } ::slotted(span) { display: block; width: 40px; height: 22px; margin: 0; padding: 0; }');
    root.adoptedStyleSheets = [this._sheet];
    root.innerHTML = '<slot name="title"></slot>';
  }
}
customElements.define('x-box', XBox);
var ds = new CSSStyleSheet();
ds.replaceSync('#p { display: flex; width: 90px; height: 30px; margin: 0; padding: 0; }');
document.adoptedStyleSheets = [ds];
window.__ds = ds;
var t = document.createElement('template');
t.innerHTML = '<?lit$1$><slot name="title"></slot>';
var c = t.content.firstChild;
window.__marker = [c && c.nodeType, c && c.data, c && c.nextSibling && c.nextSibling.localName].join('|');
var h = document.getElementById('h');
var root = h.shadowRoot;
var slot = root.querySelector('slot');
var assigned = slot.assignedElements();
window.__shape = [
  root.adoptedStyleSheets.length,
  root.querySelectorAll('*').length,
  root.querySelectorAll('style').length,
  slot.getAttribute('name'),
  assigned.length && assigned[0].id,
  document.adoptedStyleSheets.length
].join('|');
"#,
        )
        .expect("define");
    assert_eq!(js(&mut engine, view, "window.__marker"), "8|?lit$1$|slot");
    assert_eq!(js(&mut engine, view, "window.__shape"), "1|1|0|title|t|1");
    assert_eq!(
        js(
            &mut engine,
            view,
            "var h = document.getElementById('h'), t = document.getElementById('t'), p = document.getElementById('p'); var hr = h.getBoundingClientRect(), tr = t.getBoundingClientRect(), pr = p.getBoundingClientRect(); [getComputedStyle(h).display, Math.round(hr.width), Math.round(hr.height), Math.round(tr.width), Math.round(tr.height), getComputedStyle(p).display, Math.round(pr.width), Math.round(pr.height)].join('|')"
        ),
        "flex|180|64|40|22|flex|90|30"
    );
    engine
        .execute_script(
            view,
            r#"document.getElementById('h')._sheet.replaceSync(':host { display: block; width: 50px; height: 20px; margin: 0; padding: 0; }'); window.__ds.replaceSync('#p { display: block; width: 40px; height: 10px; margin: 0; padding: 0; }');"#,
        )
        .expect("replaceSync");
    assert_eq!(
        js(
            &mut engine,
            view,
            "var h = document.getElementById('h'), p = document.getElementById('p'); var hr = h.getBoundingClientRect(), pr = p.getBoundingClientRect(); [getComputedStyle(h).display, Math.round(hr.width), Math.round(hr.height), getComputedStyle(p).display, Math.round(pr.width), Math.round(pr.height)].join('|')"
        ),
        "block|50|20|block|40|10"
    );
}
