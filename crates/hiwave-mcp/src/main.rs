//! HiWave MCP — the RustKit engine, exposed to agents over stdio.
//!
//! Phase 0 of `HIWAVE_MCP_PLAN.md`. Three properties matter, and they are the
//! reasons this is a crate rather than a wrapper around `parity-capture`:
//!
//! 1. **In-process.** It calls `rustkit-engine` directly, so an agent can ask
//!    the engine what it computed — not just look at what it painted. That is
//!    the thing chrome-mcp structurally cannot do, because nobody owns Chrome.
//! 2. **Persistent.** `parity-capture` spawns, renders, exits. One page load
//!    here serves many queries, which is what makes interactive diagnosis
//!    cheap enough to actually do.
//! 3. **No new dependencies.** MCP is JSON-RPC 2.0 over stdio with a small
//!    handshake; that is ~150 lines. This workspace keeps its dependency list
//!    tight and its CI compiles exactly one crate — adding an SDK to pull that
//!    off would cost more than it saves.
//!
//! Protocol: newline-delimited JSON-RPC 2.0 on stdin/stdout. Every diagnostic
//! goes to stderr, because a stray stdout write corrupts the stream.
//!
//! Two things a caller should know, both deliberate:
//!
//! - **Screenshots are PPM, not PNG.** `capture_frame` is the engine's own
//!   capture path and the parity harness already speaks PPM, so converting
//!   here would add a dependency and a second encoder to disagree with. The
//!   tool result names the format; convert downstream if you need to.
//! - **Trust boundary: `hiwave_open { path }` reads any file the process can
//!   read.** This server is a LOCAL developer tool driven by an agent already
//!   running as the developer, so that is the same authority the agent has
//!   anyway — it is not a sandbox and must not be exposed over a socket or
//!   run as a service. If that ever changes, the path arm needs a root jail
//!   before anything else does.

use std::fs;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use rustkit_engine::{Engine, EngineBuilder, EngineConfig};
use rustkit_viewhost::Bounds;
use serde_json::{json, Value};

const PROTOCOL_VERSION: &str = "2024-11-05";

/// Where `hiwave_diff` looks for cases. Baked to this crate's directory so the
/// tool works from any cwd; `HIWAVE_MCP_CASES` overrides it for a checkout in
/// another location.
fn cases_dir() -> PathBuf {
    std::env::var_os("HIWAVE_MCP_CASES")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("cases"))
}

/// A required argument that names a directory entry, not a path.
///
/// `case` and `reference` are joined onto a directory, so `..` or a separator
/// would let a caller read outside the case tree. This server already reads any
/// file the developer can read via `hiwave_open { path }` — deliberately, see
/// the module header — but that is an explicit argument, and a name silently
/// escaping its directory is a different thing.
fn name_arg(args: &Value, key: &str) -> Result<String, String> {
    let value = args
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("`{key}` is required"))?;
    if value.is_empty()
        || value.contains(['/', '\\'])
        || value.contains("..")
        || value.starts_with('.')
    {
        return Err(format!("`{key}` must be a plain name, got {value:?}"));
    }
    Ok(value.to_string())
}

/// Resolve a dotted path with array indices — `root.children[0].border_box.width`.
fn resolve<'a>(doc: &'a Value, path: &str) -> Option<&'a Value> {
    let mut cur = doc;
    for segment in path.split('.') {
        if segment.is_empty() {
            return None;
        }
        let mut parts = segment.split('[');
        let name = parts.next().unwrap();
        if !name.is_empty() {
            cur = cur.get(name)?;
        }
        for index in parts {
            let index = index.strip_suffix(']')?;
            cur = cur.get(index.parse::<usize>().ok()?)?;
        }
    }
    Some(cur)
}

/// Build a headless engine, load `html`, render it.
///
/// Shared by `hiwave_open` and `hiwave_diff` on purpose: a diff that configured
/// the engine even slightly differently from the session would report the
/// difference between two setups as a difference in the engine.
fn build_view(
    html: &str,
    width: u32,
    height: u32,
) -> Result<(Engine, rustkit_engine::EngineViewId), String> {
    let mut engine = EngineBuilder::new()
        .with_config(EngineConfig::for_parity_testing())
        .user_agent("HiWaveMCP/0.1")
        .javascript_enabled(false)
        .build()
        .map_err(|e| format!("engine build failed: {e:?}"))?;

    let view_id = engine
        .create_headless_view(Bounds { x: 0, y: 0, width, height })
        .map_err(|e| format!("headless view failed: {e:?}"))?;

    // Always on here, and it must precede the load: the cascade records as it
    // runs, so arming it afterwards would leave `hiwave_style` with an empty
    // trace for a page that is plainly loaded. This server is a diagnostic
    // tool — recording provenance is the job, not overhead.
    engine.set_style_recording(true);

    engine
        .load_html(view_id, html)
        .map_err(|e| format!("load failed: {e:?}"))?;
    engine
        .render_view(view_id)
        .map_err(|e| format!("render failed: {e:?}"))?;
    Ok((engine, view_id))
}

/// One loaded page: the engine, its headless view, and how it was created.
struct Session {
    engine: Engine,
    view_id: rustkit_engine::EngineViewId,
    width: u32,
    height: u32,
    source: String,
}

#[derive(Default)]
struct Server {
    session: Option<Session>,
    scratch: Option<PathBuf>,
}

impl Server {
    /// A private directory for frame/layout dumps, created on first use.
    fn scratch(&mut self) -> io::Result<PathBuf> {
        if self.scratch.is_none() {
            let dir = std::env::temp_dir().join(format!("hiwave-mcp-{}", std::process::id()));
            fs::create_dir_all(&dir)?;
            self.scratch = Some(dir);
        }
        Ok(self.scratch.clone().unwrap())
    }

    fn open(&mut self, args: &Value) -> Result<Value, String> {
        let width = args.get("width").and_then(Value::as_u64).unwrap_or(1280) as u32;
        let height = args.get("height").and_then(Value::as_u64).unwrap_or(800) as u32;

        // `html` is inline source; `path` is a file. Exactly one is required —
        // silently preferring one would make a typo look like a render bug.
        let (html, source) = match (args.get("html").and_then(Value::as_str),
                                    args.get("path").and_then(Value::as_str)) {
            (Some(_), Some(_)) => {
                return Err("pass either `html` or `path`, not both".into())
            }
            (Some(h), None) => (h.to_string(), "<inline>".to_string()),
            (None, Some(p)) => {
                let content = fs::read_to_string(p)
                    .map_err(|e| format!("cannot read {p}: {e}"))?;
                (content, p.to_string())
            }
            (None, None) => return Err("one of `html` or `path` is required".into()),
        };

        let (engine, view_id) = build_view(&html, width, height)?;

        self.session = Some(Session { engine, view_id, width, height, source: source.clone() });
        Ok(json!({ "loaded": source, "width": width, "height": height }))
    }

    fn session_mut(&mut self) -> Result<&mut Session, String> {
        self.session
            .as_mut()
            .ok_or_else(|| "no page loaded — call hiwave_open first".to_string())
    }

    fn layout(&mut self, _args: &Value) -> Result<Value, String> {
        let dir = self.scratch().map_err(|e| e.to_string())?;
        let out = dir.join("layout.json");
        let s = self.session_mut()?;
        s.engine
            .export_layout_json(s.view_id, out.to_str().unwrap())
            .map_err(|e| format!("layout export failed: {e:?}"))?;
        let raw = fs::read_to_string(&out).map_err(|e| e.to_string())?;
        // Returned as parsed JSON, not a string: an agent should be able to
        // walk the tree without re-parsing a blob it was just handed.
        serde_json::from_str(&raw).map_err(|e| format!("layout JSON unreadable: {e}"))
    }

    /// The paint commands the engine built from the layout tree, in order.
    ///
    /// Paired with `hiwave_layout` this is the whole point of the crate: with
    /// both, an agent can ask whether a wrong pixel came from the box being
    /// computed wrong or from the box being painted wrong, instead of
    /// inferring the answer from the pixel.
    fn display_list(&mut self, _args: &Value) -> Result<Value, String> {
        let dir = self.scratch().map_err(|e| e.to_string())?;
        let out = dir.join("display_list.json");
        let s = self.session_mut()?;
        s.engine
            .export_display_list_json(s.view_id, out.to_str().unwrap())
            .map_err(|e| format!("display list export failed: {e:?}"))?;
        let raw = fs::read_to_string(&out).map_err(|e| e.to_string())?;
        serde_json::from_str(&raw).map_err(|e| format!("display list JSON unreadable: {e}"))
    }

    /// The cascade behind the computed style: winning rule, origin, and the
    /// declarations that lost.
    ///
    /// Layout and the display list both report a CONSEQUENCE. When a rule is
    /// parsed and matched and then overridden, neither can say so — the box
    /// is simply the wrong size and the cause is invisible. This is the tool
    /// that answers "why", which is why it returns the overridden
    /// declarations rather than only the value that survived.
    fn style(&mut self, args: &Value) -> Result<Value, String> {
        let selector = args
            .get("selector")
            .and_then(Value::as_str)
            .ok_or("`selector` is required")?
            .to_string();
        let dir = self.scratch().map_err(|e| e.to_string())?;
        let out = dir.join("style.json");
        let s = self.session_mut()?;
        s.engine
            .export_style_json(s.view_id, &selector, out.to_str().unwrap())
            .map_err(|e| format!("style export failed: {e:?}"))?;
        let raw = fs::read_to_string(&out).map_err(|e| e.to_string())?;
        serde_json::from_str(&raw).map_err(|e| format!("style JSON unreadable: {e}"))
    }

    /// Stage-wise agreement between what this engine computes and a committed
    /// reference — the join the other three tools were built to feed.
    ///
    /// The artefacts it compares are TEXT: layout trees and display lists, not
    /// framebuffers. That is the whole point. A porting seat asking "did my
    /// port compute the same thing the reference computed" cannot answer it
    /// with a pixel capture on a machine whose GPU capture is untrusted, and a
    /// unit-test count can be cfg-gated out without anyone noticing. A stage
    /// diff answers it directly and is deterministic on any machine.
    ///
    /// It runs the case in its OWN engine rather than the open session, so a
    /// diff never depends on what someone happened to load first, and never
    /// disturbs it.
    fn diff(&mut self, args: &Value) -> Result<Value, String> {
        let case = name_arg(args, "case")?;
        let reference = name_arg(args, "reference")?;
        let stage = args
            .get("stage")
            .and_then(Value::as_str)
            .unwrap_or("layout")
            .to_string();
        if stage != "layout" && stage != "display_list" {
            return Err(format!(
                "unknown stage `{stage}` — this tool diffs `layout` or `display_list`. \
                 Those are the deterministic text stages; `style` and `screenshot` are not \
                 diffable here yet."
            ));
        }

        let dir = cases_dir().join(&case);
        if !dir.is_dir() {
            return Err(format!("no such case `{case}` — expected {}", dir.display()));
        }
        let page = dir.join("page.html");
        let ref_path = dir.join(format!("{reference}.{stage}.json"));
        let raw = fs::read_to_string(&ref_path).map_err(|e| {
            format!(
                "no reference `{reference}` for case `{case}` at stage `{stage}` \
                 ({}): {e}",
                ref_path.display()
            )
        })?;
        let doc: Value = serde_json::from_str(&raw)
            .map_err(|e| format!("reference {} is not readable JSON: {e}", ref_path.display()))?;

        // A reference that does not say what it is cannot be trusted to say
        // what the engine should be. Every field below is required.
        let kind = doc
            .get("kind")
            .and_then(Value::as_str)
            .ok_or("reference has no `kind`")?;
        if kind != "expectations" {
            return Err(format!(
                "reference kind `{kind}` is not implemented — this tool reads \
                 `expectations` (a list of hand-derived path/value pairs). Full committed \
                 captures are the other reference kind the plan calls for and they are NOT \
                 supported yet."
            ));
        }
        if doc.get("stage").and_then(Value::as_str) != Some(stage.as_str()) {
            return Err(format!(
                "reference {} declares stage {:?}, not `{stage}`",
                ref_path.display(),
                doc.get("stage")
            ));
        }
        let expectations = doc
            .get("expect")
            .and_then(Value::as_array)
            .ok_or("reference has no `expect` array")?;

        // Layout depends on the viewport, so the reference states the one its
        // numbers were derived at rather than inheriting whatever is open.
        let vp = doc.get("viewport").ok_or("reference has no `viewport`")?;
        let width = vp.get("width").and_then(Value::as_u64).ok_or("viewport.width")? as u32;
        let height = vp.get("height").and_then(Value::as_u64).ok_or("viewport.height")? as u32;

        let html = fs::read_to_string(&page)
            .map_err(|e| format!("cannot read {}: {e}", page.display()))?;
        let live = self.export_stage(&html, &stage, width, height)?;

        let mut differences = Vec::new();
        for (i, want) in expectations.iter().enumerate() {
            let path = want
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("expect[{i}] has no `path`"))?;
            let expected = want
                .get("value")
                .ok_or_else(|| format!("expect[{i}] has no `value`"))?;
            let why = want.get("why").cloned().unwrap_or(Value::Null);
            let tolerance = want.get("tolerance").and_then(Value::as_f64);

            match resolve(&live, path) {
                None => differences.push(json!({
                    "path": path, "expected": expected, "actual": null, "why": why,
                    "note": "no such path in the live export",
                })),
                Some(actual) => {
                    // Compared as numbers when both are numbers, so `432` in a
                    // reference and `432.0` from the engine are not a false diff.
                    let same = match (expected.as_f64(), actual.as_f64()) {
                        (Some(e), Some(a)) => match tolerance {
                            Some(t) => (e - a).abs() <= t,
                            None => e == a,
                        },
                        _ => expected == actual,
                    };
                    if !same {
                        differences.push(json!({
                            "path": path, "expected": expected, "actual": actual,
                            "why": why, "tolerance": tolerance,
                        }));
                    }
                }
            }
        }

        Ok(json!({
            "case": case,
            "stage": stage,
            "reference": reference,
            "reference_kind": kind,
            "reference_origin": doc.get("origin").cloned().unwrap_or(Value::Null),
            "viewport": { "width": width, "height": height },
            "checked": expectations.len(),
            "differences": differences.len(),
            "agrees": differences.is_empty(),
            "disagreements": differences,
        }))
    }

    /// Load `html` into a throwaway engine and return one stage's export.
    fn export_stage(
        &mut self,
        html: &str,
        stage: &str,
        width: u32,
        height: u32,
    ) -> Result<Value, String> {
        let dir = self.scratch().map_err(|e| e.to_string())?;
        let out = dir.join(format!("diff.{stage}.json"));
        let (engine, view_id) = build_view(html, width, height)?;
        match stage {
            "layout" => engine.export_layout_json(view_id, out.to_str().unwrap()),
            _ => engine.export_display_list_json(view_id, out.to_str().unwrap()),
        }
        .map_err(|e| format!("{stage} export failed: {e:?}"))?;
        let raw = fs::read_to_string(&out).map_err(|e| e.to_string())?;
        serde_json::from_str(&raw).map_err(|e| format!("{stage} JSON unreadable: {e}"))
    }

    fn screenshot(&mut self, args: &Value) -> Result<Value, String> {
        let dir = self.scratch().map_err(|e| e.to_string())?;
        let out = match args.get("path").and_then(Value::as_str) {
            Some(p) => PathBuf::from(p),
            None => dir.join("frame.ppm"),
        };
        let s = self.session_mut()?;
        s.engine
            .capture_frame(s.view_id, out.to_str().unwrap())
            .map_err(|e| format!("capture failed: {e:?}"))?;
        Ok(json!({
            "path": out.to_string_lossy(),
            "width": s.width,
            "height": s.height,
            "format": "ppm",
        }))
    }

    fn status(&mut self, _args: &Value) -> Result<Value, String> {
        Ok(match &self.session {
            Some(s) => json!({
                "loaded": true, "source": s.source,
                "width": s.width, "height": s.height,
            }),
            None => json!({ "loaded": false }),
        })
    }

    fn call_tool(&mut self, name: &str, args: &Value) -> Result<Value, String> {
        match name {
            "hiwave_open" => self.open(args),
            "hiwave_layout" => self.layout(args),
            "hiwave_display_list" => self.display_list(args),
            "hiwave_style" => self.style(args),
            "hiwave_diff" => self.diff(args),
            "hiwave_screenshot" => self.screenshot(args),
            "hiwave_status" => self.status(args),
            other => Err(format!("unknown tool: {other}")),
        }
    }
}

fn tool_list() -> Value {
    json!({ "tools": [
        {
            "name": "hiwave_open",
            "description": "Load a page into a persistent headless RustKit engine and render it. \
                            Subsequent tool calls query THIS page until the next open.",
            "inputSchema": { "type": "object", "properties": {
                "html":   { "type": "string", "description": "Inline HTML source" },
                "path":   { "type": "string", "description": "Path to an HTML file" },
                "width":  { "type": "integer", "description": "Viewport width (default 1280)" },
                "height": { "type": "integer", "description": "Viewport height (default 800)" }
            }}
        },
        {
            "name": "hiwave_layout",
            "description": "The engine's computed layout tree for the loaded page, as JSON. \
                            This is what RustKit decided, not what it painted — use it to \
                            attribute a diff to a stage instead of guessing from pixels.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "hiwave_display_list",
            "description": "The engine's paint commands for the loaded page, flat and in paint \
                            order. Pair it with hiwave_layout to attribute a visual bug to a \
                            stage: if the layout box is right and the paint rect is wrong, the \
                            bug is in paint. Later commands cover earlier ones. Commands the \
                            exporter has not modelled carry \"modelled\": false and a debug dump.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "hiwave_style",
            "description": "The cascade for the elements matching a simple selector: computed \
                            values, and per declared property the WINNING rule (selector, \
                            specificity, origin) plus every declaration it overrode. Use it when \
                            a box is the wrong size and layout looks right — the cause is usually \
                            a rule that matched and then lost. Queries accept simple selectors \
                            only (`tag`, `.class`, `#id`, `tag.class`) and are refused otherwise. \
                            `origin` is author or author-inline: the UA stylesheet is hardcoded, \
                            not parsed, so it has no rule to cite. `!important` is reported but \
                            NOT honoured by this cascade — an important declaration that lost is \
                            an engine bug the tool will show you.",
            "inputSchema": { "type": "object", "properties": {
                "selector": { "type": "string", "description": "Simple selector, e.g. \".hero\"" }
            }, "required": ["selector"] }
        },
        {
            "name": "hiwave_diff",
            "description": "Compare what this engine computes for a committed case against a \
                            committed reference, at one stage, and report every field that \
                            disagrees with both values. Stages are `layout` and `display_list` \
                            — text artefacts, so the answer is deterministic on any machine and \
                            does not depend on trusting a GPU capture. The case is run in its \
                            own engine, so a diff neither depends on nor disturbs the open page. \
                            References today are `expectations`: hand-derived path/value pairs \
                            with the derivation recorded alongside. Full committed captures are \
                            NOT supported yet. `agrees` is the answer; `disagreements` is why.",
            "inputSchema": { "type": "object", "properties": {
                "case":      { "type": "string", "description": "Case name under crates/hiwave-mcp/cases/" },
                "stage":     { "type": "string", "description": "`layout` (default) or `display_list`" },
                "reference": { "type": "string", "description": "Reference name, e.g. \"spec\"" }
            }, "required": ["case", "reference"] }
        },
        {
            "name": "hiwave_screenshot",
            "description": "Capture the rendered frame to a PPM file and return its path.",
            "inputSchema": { "type": "object", "properties": {
                "path": { "type": "string", "description": "Output path (default: a scratch file)" }
            }}
        },
        {
            "name": "hiwave_status",
            "description": "Whether a page is loaded, and which one.",
            "inputSchema": { "type": "object", "properties": {} }
        }
    ]})
}

fn respond(id: Value, result: Result<Value, String>) -> Value {
    match result {
        Ok(value) => json!({ "jsonrpc": "2.0", "id": id, "result": value }),
        // MCP convention: a tool that fails reports isError in its RESULT
        // rather than a protocol-level error, so the agent sees the message.
        Err(message) => json!({ "jsonrpc": "2.0", "id": id, "result": {
            "content": [{ "type": "text", "text": message }],
            "isError": true
        }}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> Value {
        json!({ "root": { "children": [
            { "border_box": { "width": 432.0 }, "children": [{ "x": 16.0 }] }
        ]}, "commands": [{ "op": "text" }] })
    }

    #[test]
    fn resolve_walks_names_and_indices() {
        let doc = tree();
        assert_eq!(
            resolve(&doc, "root.children[0].border_box.width"),
            Some(&json!(432.0))
        );
        assert_eq!(
            resolve(&doc, "root.children[0].children[0].x"),
            Some(&json!(16.0))
        );
        assert_eq!(resolve(&doc, "commands[0].op"), Some(&json!("text")));
    }

    #[test]
    fn resolve_reports_a_missing_path_rather_than_guessing() {
        // Every one of these must be None, not a nearby value: a diff that
        // silently resolved a typo'd path would report agreement it never
        // checked, which is worse than reporting nothing.
        let doc = tree();
        for path in [
            "root.children[1].border_box.width", // index past the end
            "root.children[0].margin_box.width", // no such field
            "root.children[0].border_box.hight", // typo
            "root..width",                       // empty segment
            "commands[9].op",
            "",
        ] {
            assert_eq!(resolve(&doc, path), None, "{path} should not resolve");
        }
    }

    #[test]
    fn name_arg_refuses_anything_that_escapes_the_case_directory() {
        assert_eq!(name_arg(&json!({ "case": "hero" }), "case").unwrap(), "hero");
        for bad in ["../etc", "a/b", "a\\b", ".hidden", "", "..", "x/../.."] {
            assert!(
                name_arg(&json!({ "case": bad }), "case").is_err(),
                "{bad:?} should be refused"
            );
        }
        assert!(name_arg(&json!({}), "case").is_err(), "missing is required");
    }
}

fn main() {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "warn".to_string());
    tracing_subscriber::fmt()
        .with_env_filter(&filter)
        .with_writer(io::stderr) // stdout belongs to the protocol
        .init();

    let mut server = Server::default();
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) if !l.trim().is_empty() => l,
            Ok(_) => continue,
            Err(e) => {
                eprintln!("stdin closed: {e}");
                break;
            }
        };

        let request: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("unparseable request: {e}");
                continue;
            }
        };

        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let method = request.get("method").and_then(Value::as_str).unwrap_or("");
        let params = request.get("params").cloned().unwrap_or(json!({}));

        let response = match method {
            "initialize" => Some(respond(id, Ok(json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "hiwave-mcp", "version": env!("CARGO_PKG_VERSION") }
            })))),
            // Notifications carry no id and MUST NOT be answered.
            "notifications/initialized" => None,
            "tools/list" => Some(respond(id, Ok(tool_list()))),
            "tools/call" => {
                let name = params.get("name").and_then(Value::as_str).unwrap_or("");
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                let outcome = server.call_tool(name, &args).map(|value| json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&value).unwrap_or_default()
                    }]
                }));
                Some(respond(id, outcome))
            }
            other => Some(json!({ "jsonrpc": "2.0", "id": id, "error": {
                "code": -32601, "message": format!("method not found: {other}")
            }})),
        };

        if let Some(response) = response {
            if writeln!(stdout, "{response}").and_then(|_| stdout.flush()).is_err() {
                break;
            }
        }
    }
}
