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

use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use rustkit_engine::{Engine, EngineBuilder, EngineConfig};
use rustkit_viewhost::Bounds;
use serde_json::{json, Value};

const PROTOCOL_VERSION: &str = "2024-11-05";

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

        let mut engine = EngineBuilder::new()
            .with_config(EngineConfig::for_parity_testing())
            .user_agent("HiWaveMCP/0.1")
            .javascript_enabled(false)
            .build()
            .map_err(|e| format!("engine build failed: {e:?}"))?;

        let view_id = engine
            .create_headless_view(Bounds { x: 0, y: 0, width, height })
            .map_err(|e| format!("headless view failed: {e:?}"))?;

        engine
            .load_html(view_id, &html)
            .map_err(|e| format!("load failed: {e:?}"))?;
        engine
            .render_view(view_id)
            .map_err(|e| format!("render failed: {e:?}"))?;

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

#[allow(dead_code)]
fn _unused(_: &Path) {}
