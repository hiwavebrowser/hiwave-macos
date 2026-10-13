//! HiWave Inspector, Console and Network half: the per-view record of what a
//! page load did, in order, for an inspector window, the site board and
//! debugging agents to read.
//!
//! Each view owns one [`Inspector`]. It records only while something listens:
//! the view's ring buffer is on ([`InspectorConfig::buffer`]) or a sink is
//! attached ([`Inspector::add_sink`]). Otherwise every recording call returns
//! after one atomic load, before any event is built or any string formatted.
//!
//! Privacy (the log is pasted into public issues and PRs): a URL in an event
//! keeps its scheme, host, port and path, never its user name, password,
//! query or fragment (a flag keeps the query and fragment); a `data:` URL
//! keeps only its MIME type. No header, request or response body, form value
//! or storage is recorded. A console message is, by default, each argument's
//! type and a string's length only (pages log cookies, form values and their
//! own URL); with [`InspectorConfig::keep_console_text`] it is the text,
//! capped, URLs in it scrubbed, never interpreted.
//!
//! The buffer is behind a mutex shared with the embedder ([`Engine::inspector`]
//! hands out an `Arc`), so another thread can take a snapshot while the page's
//! script is running. Reading runs no script.
//!
//! [`Engine::inspector`]: crate::Engine::inspector

use std::collections::VecDeque;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use url::Url;

/// Events a view's ring buffer holds by default.
pub const DEFAULT_CAPACITY: usize = 4096;
/// Longest console message kept, in characters.
pub const MAX_MESSAGE_CHARS: usize = 1024;
/// Longest error stack kept, in characters.
pub const MAX_STACK_CHARS: usize = 4096;
/// Longest short text (an error name, a reason, an interface or member name).
pub const MAX_SHORT_CHARS: usize = 256;
/// Most distinct unsupported members recorded per page.
pub const MAX_UNSUPPORTED_MEMBERS: usize = 512;

/// How a view records (part of `EngineConfig`).
#[derive(Debug, Clone)]
pub struct InspectorConfig {
    /// Keep the last `capacity` events in the view's ring buffer.
    pub buffer: bool,
    /// Size of the ring buffer; past it the oldest event is dropped and
    /// counted.
    pub capacity: usize,
    /// Keep the query string and fragment of URLs in events. Off by default:
    /// they carry session ids and tokens.
    pub keep_url_query: bool,
    /// Keep the text of console messages. Off by default: a console line
    /// is then each argument's type, and a string's length (`<string
    /// len=12>`), since pages log cookies, form values and their own URL.
    /// When on, URLs in the text are still scrubbed as event URLs are.
    pub keep_console_text: bool,
}

impl Default for InspectorConfig {
    fn default() -> Self {
        Self {
            buffer: false,
            capacity: DEFAULT_CAPACITY,
            keep_url_query: false,
            keep_console_text: false,
        }
    }
}

/// A console method's level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsoleLevel {
    Log,
    Info,
    Warn,
    Error,
    Debug,
}

/// A place in a script: its URL (or `inline#n` for the page's n-th
/// `<script>` when inline; the line is then counted from the start of the
/// script's text), and a 1-based line and column.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceLocation {
    pub url: String,
    pub line: u32,
    pub column: u32,
}

/// Why a navigation stopped before its load finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndReason {
    /// `Engine::stop`, or another navigation started in the view.
    NavigatedAway,
    /// The document could not be fetched or was refused.
    Failed,
}

/// One thing that happened. Serialized with a `type` tag and the fields in
/// declaration order; new fields are only ever appended, and a field that
/// is `null` today may hold a value later.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InspectorEvent {
    /// A navigation began (`load_url`, or `load_html` with no URL). The
    /// event times that follow count from here.
    NavigationStart { url: Option<String> },
    /// The document's response arrived and replaced the previous document.
    NavigationCommit { url: Option<String>, status: Option<u16> },
    /// The load finished: scripts, lifecycle events and the timers up to
    /// the horizon ran. `scripts_over_budget` counts the scripts not run
    /// (or stopped) because the page's script budget was spent.
    LoadFinished { url: Option<String>, scripts_over_budget: u32 },
    /// The load ended without finishing.
    LoadEndedEarly { url: Option<String>, reason: EndReason, detail: String },
    /// A `console.*` call.
    Console {
        level: ConsoleLevel,
        message: String,
        /// The message was longer than [`MAX_MESSAGE_CHARS`] and was cut.
        truncated: bool,
        location: Option<SourceLocation>,
    },
    /// An exception no script caught, or a promise rejected with no handler.
    ScriptError {
        /// `TypeError`, `Error`, ...; empty when a non-error was thrown.
        name: String,
        message: String,
        location: Option<SourceLocation>,
        stack: Option<String>,
        unhandled_rejection: bool,
        /// Which of the page's scripts it came from: a script URL,
        /// `inline#n`, or the engine step that ran it (`event:load`,
        /// `timers`, `network`, ...).
        script: String,
    },
    /// Page script used a Web API member the engine does not implement.
    /// Recorded once per page per member; `Inspector::unsupported_apis`
    /// and the summary carry the final counts.
    UnsupportedApi {
        interface: String,
        member: String,
        location: Option<SourceLocation>,
    },
    /// The view's first layout of this document.
    FirstLayout,
    /// The view's first frame of this document.
    FirstPaint,
    /// A relayout and the cause it was laid out for (the value the
    /// `relayout cause=` log line shows).
    Relayout { cause: String, took_ms: f64 },
}

impl InspectorEvent {
    /// The `type` tag.
    pub fn kind(&self) -> &'static str {
        match self {
            InspectorEvent::NavigationStart { .. } => "navigation_start",
            InspectorEvent::NavigationCommit { .. } => "navigation_commit",
            InspectorEvent::LoadFinished { .. } => "load_finished",
            InspectorEvent::LoadEndedEarly { .. } => "load_ended_early",
            InspectorEvent::Console { .. } => "console",
            InspectorEvent::ScriptError { .. } => "script_error",
            InspectorEvent::UnsupportedApi { .. } => "unsupported_api",
            InspectorEvent::FirstLayout => "first_layout",
            InspectorEvent::FirstPaint => "first_paint",
            InspectorEvent::Relayout { .. } => "relayout",
        }
    }
}

/// An event with its place in the view's sequence and its time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InspectorRecord {
    /// 0-based, per view, never reused.
    pub seq: u64,
    /// Milliseconds since the current navigation started (monotonic clock).
    pub t_ms: f64,
    #[serde(flatten)]
    pub event: InspectorEvent,
}

/// What [`Inspector::snapshot_since`] returns.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InspectorSnapshot {
    /// The buffered records with `seq >= since`, oldest first.
    pub records: Vec<InspectorRecord>,
    /// The `seq` the next record will get: pass it as `since` next time.
    pub next_seq: u64,
    /// Records the ring buffer has dropped since the view was made.
    pub dropped: u64,
}

/// Somewhere else records go, besides the ring buffer (a file, a socket).
/// Called in order, under the view's inspector lock: keep it quick.
pub trait InspectorSink: Send {
    fn record(&mut self, record: &InspectorRecord);
    fn flush(&mut self) {}
}

/// One unsupported member, with how often page script reached it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnsupportedApiCount {
    pub interface: String,
    pub member: String,
    pub count: u64,
    pub location: Option<SourceLocation>,
}

struct State {
    buffer: bool,
    capacity: usize,
    records: VecDeque<InspectorRecord>,
    next_seq: u64,
    dropped: u64,
    navigation_started: Instant,
    sinks: Vec<Box<dyn InspectorSink>>,
    unsupported: Vec<UnsupportedApiCount>,
    laid_out: bool,
    painted: bool,
    /// The names page scripts are evaluated under that are not their URL
    /// (`script#3`), and the URL each stands for (as events show it).
    aliases: Vec<(String, String)>,
}

/// A view's recorder and ring buffer. See the module docs.
pub struct Inspector {
    active: AtomicBool,
    keep_url_query: bool,
    keep_console_text: bool,
    state: Mutex<State>,
}

impl Inspector {
    pub fn new(config: &InspectorConfig) -> Arc<Self> {
        Arc::new(Self {
            active: AtomicBool::new(config.buffer),
            keep_url_query: config.keep_url_query,
            keep_console_text: config.keep_console_text,
            state: Mutex::new(State {
                buffer: config.buffer,
                capacity: config.capacity.max(1),
                records: VecDeque::new(),
                next_seq: 0,
                dropped: 0,
                navigation_started: Instant::now(),
                sinks: Vec::new(),
                unsupported: Vec::new(),
                laid_out: false,
                painted: false,
                aliases: Vec::new(),
            }),
        })
    }

    /// Whether anything listens. Every recording path checks this first.
    #[inline]
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Relaxed)
    }

    fn state(&self) -> MutexGuard<'_, State> {
        // A sink that panicked leaves the lock poisoned; the buffer is
        // still consistent (records are pushed whole).
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn refresh_active(&self, state: &State) {
        self.active
            .store(state.buffer || !state.sinks.is_empty(), Ordering::Relaxed);
    }

    /// Turn the ring buffer on or off. Turning it off empties it.
    pub fn set_buffer_enabled(&self, on: bool) {
        let mut state = self.state();
        state.buffer = on;
        if !on {
            state.records.clear();
        }
        self.refresh_active(&state);
    }

    /// Send every record from now on to `sink` as well.
    pub fn add_sink(&self, sink: Box<dyn InspectorSink>) {
        let mut state = self.state();
        state.sinks.push(sink);
        self.refresh_active(&state);
    }

    /// Flush and drop every sink.
    pub fn clear_sinks(&self) {
        let mut state = self.state();
        for sink in &mut state.sinks {
            sink.flush();
        }
        state.sinks.clear();
        self.refresh_active(&state);
    }

    /// Flush every sink.
    pub fn flush(&self) {
        for sink in &mut self.state().sinks {
            sink.flush();
        }
    }

    /// The buffered records from `since` on. Takes the lock briefly; runs
    /// no page script, so it can be called from any thread at any time.
    pub fn snapshot_since(&self, since: u64) -> InspectorSnapshot {
        let state = self.state();
        InspectorSnapshot {
            records: state.records.iter().filter(|r| r.seq >= since).cloned().collect(),
            next_seq: state.next_seq,
            dropped: state.dropped,
        }
    }

    /// The current page's unsupported members, in the order first seen.
    pub fn unsupported_apis(&self) -> Vec<UnsupportedApiCount> {
        self.state().unsupported.clone()
    }

    /// Record `event` if anything listens. Prefer [`emit_with`](Self::emit_with)
    /// where building the event costs anything.
    pub fn emit(&self, event: InspectorEvent) {
        if self.is_active() {
            self.push(event);
        }
    }

    /// Record the event `build` makes, calling it only if anything listens.
    #[inline]
    pub fn emit_with(&self, build: impl FnOnce(&Self) -> InspectorEvent) {
        if self.is_active() {
            let event = build(self);
            self.push(event);
        }
    }

    fn push(&self, event: InspectorEvent) {
        let mut state = self.state();
        Self::push_locked(&mut state, event);
    }

    fn push_locked(state: &mut State, event: InspectorEvent) {
        let record = InspectorRecord {
            seq: state.next_seq,
            t_ms: round_ms(state.navigation_started.elapsed().as_secs_f64() * 1000.0),
            event,
        };
        state.next_seq += 1;
        for sink in &mut state.sinks {
            sink.record(&record);
        }
        if state.buffer {
            if state.records.len() >= state.capacity {
                state.records.pop_front();
                state.dropped += 1;
            }
            state.records.push_back(record);
        }
    }

    /// A navigation began: restart the clock and the per-page tables, then
    /// record its start.
    pub fn navigation_started(&self, url: Option<&Url>) {
        if !self.is_active() {
            return;
        }
        let url = url.map(|u| self.url(u));
        let mut state = self.state();
        state.navigation_started = Instant::now();
        state.unsupported.clear();
        state.laid_out = false;
        state.painted = false;
        state.aliases.clear();
        Self::push_locked(&mut state, InspectorEvent::NavigationStart { url });
    }

    /// The view laid this document out: `FirstLayout` the first time.
    pub fn laid_out(&self) {
        if !self.is_active() {
            return;
        }
        let mut state = self.state();
        if !state.laid_out {
            state.laid_out = true;
            Self::push_locked(&mut state, InspectorEvent::FirstLayout);
        }
    }

    /// A relayout finished, laid out for `cause`.
    pub fn relayout(&self, cause: &dyn std::fmt::Display, took_ms: f64) {
        self.emit_with(|_| InspectorEvent::Relayout {
            cause: cause.to_string(),
            took_ms: round_ms(took_ms),
        });
    }

    /// The view presented a frame of this document: `FirstPaint` once.
    pub fn painted(&self) {
        if !self.is_active() {
            return;
        }
        let mut state = self.state();
        if state.laid_out && !state.painted {
            state.painted = true;
            Self::push_locked(&mut state, InspectorEvent::FirstPaint);
        }
    }

    /// Page script reached `interface.member`, which the engine does not
    /// implement, `count` more times. The first sighting on a page is an
    /// event; later ones only count.
    pub fn unsupported_api(&self, interface: &str, member: &str, count: u64, location: Option<SourceLocation>) {
        if !self.is_active() {
            return;
        }
        let mut state = self.state();
        if let Some(seen) = state
            .unsupported
            .iter_mut()
            .find(|u| u.interface == interface && u.member == member)
        {
            seen.count += count;
            return;
        }
        if state.unsupported.len() >= MAX_UNSUPPORTED_MEMBERS {
            return;
        }
        let interface = cap(interface, MAX_SHORT_CHARS).0;
        let member = cap(member, MAX_SHORT_CHARS).0;
        state.unsupported.push(UnsupportedApiCount {
            interface: interface.clone(),
            member: member.clone(),
            count,
            location: location.clone(),
        });
        Self::push_locked(&mut state, InspectorEvent::UnsupportedApi { interface, member, location });
    }

    /// Page script `url` is evaluated under the name `alias`: positions in
    /// it are shown with its URL.
    pub fn alias_script(&self, alias: &str, url: &str) {
        if !self.is_active() {
            return;
        }
        let url = self.url_text(url);
        self.state().aliases.push((alias.to_string(), url));
    }

    /// A position as events show it: the script's URL (an alias resolved,
    /// a URL scrubbed) or its `inline#n` label.
    pub fn location(&self, path: &str, line: u32, column: u32) -> SourceLocation {
        let url = self
            .state()
            .aliases
            .iter()
            .find(|(alias, _)| alias == path)
            .map(|(_, url)| url.clone())
            .unwrap_or_else(|| self.url_text(path));
        SourceLocation { url, line, column }
    }

    /// Engine-made text that can name scripts (an error message, a stack):
    /// each `(path:line:col)` position is shown as [`location`](Self::location)
    /// shows it, and every `http(s)://`, `ws(s)://` or `file://` URL anywhere
    /// in it (after a quote, a bracket, an `=`) is scrubbed as event URLs are.
    pub fn scrub_text(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        for (i, line) in text.split('\n').enumerate() {
            if i > 0 {
                out.push('\n');
            }
            // A position's path holds no space, so the last ` (` opens it
            // even when the path's query holds a `(`.
            let rewritten = line.rfind(" (").and_then(|space| {
                let open = space + 1;
                let inner = line[open + 1..].strip_suffix(')')?;
                let (path, l, c) = parse_position(inner)?;
                let at = self.location(&path, l, c);
                Some(format!("{}({}:{}:{})", self.scrub_urls(&line[..open]), at.url, at.line, at.column))
            });
            match rewritten {
                Some(line) => out.push_str(&line),
                None => out.push_str(&self.scrub_urls(line)),
            }
        }
        out
    }

    /// Every URL in `text` scrubbed, wherever it starts. A URL's path runs
    /// to whitespace, a quote or a bracket; once its query or fragment
    /// starts, everything up to whitespace is the query (brackets, parens
    /// and quotes are legal in it), less a trailing `:line:col)` position
    /// or closing punctuation. A `data:` URL becomes `data:<mime>,…`, its
    /// payload (which may hold spaces) dropped to the end of the line.
    fn scrub_urls(&self, text: &str) -> String {
        const SCHEMES: [&str; 6] = ["https://", "http://", "wss://", "ws://", "file://", "data:"];
        let lower = text.to_ascii_lowercase();
        let mut out = String::with_capacity(text.len());
        let mut rest = 0;
        while rest < text.len() {
            let next = SCHEMES
                .iter()
                .filter_map(|scheme| find_scheme(&lower, rest, scheme))
                .min();
            let Some(start) = next else { break };
            out.push_str(&text[rest..start]);
            if lower[start..].starts_with("data:") {
                let tail = query_tail(&text[start..]);
                let mime = text[start + 5..]
                    .split([',', ';'])
                    .next()
                    .unwrap_or("")
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || "/+-.".contains(*c))
                    .take(64)
                    .collect::<String>();
                out.push_str(&format!("data:{mime},…"));
                out.push_str(tail);
                rest = text.len();
                break;
            }
            let stop = text[start..]
                .find(|c: char| c.is_whitespace() || "'\"`<>()[]{}?#".contains(c))
                .map(|i| start + i)
                .unwrap_or(text.len());
            // `(url, kept tail, where the text resumes)`.
            let (url_end, tail, end) = if text[stop..].starts_with(['?', '#']) {
                let end = text[stop..]
                    .find(char::is_whitespace)
                    .map(|i| stop + i)
                    .unwrap_or(text.len());
                let tail = query_tail(&text[stop..end]);
                let url_end = if self.keep_url_query { end - tail.len() } else { stop };
                (url_end, tail, end)
            } else {
                // Trailing sentence punctuation is not part of the URL.
                let mut end = stop;
                while end > start && text[..end].ends_with(['.', ',', ';', ':', '!']) {
                    end -= 1;
                }
                (end, "", end)
            };
            match Url::parse(&text[start..url_end]) {
                Ok(url) => out.push_str(&self.url(&url)),
                // Not a URL after all: keep the scheme, drop what follows.
                Err(_) => out.push_str(&text[start..start + text[start..].find("//").map_or(0, |i| i + 2)]),
            }
            out.push_str(tail);
            rest = end;
        }
        out.push_str(&text[rest..]);
        out
    }

    /// A URL as events show it (see the module docs).
    pub fn url(&self, url: &Url) -> String {
        scrub_url(url, self.keep_url_query)
    }

    /// [`url`](Self::url) for text that may not be a URL (a script label
    /// such as `inline#2`, which is kept as it is).
    pub fn url_text(&self, text: &str) -> String {
        // An inline module runs as `<document>#inline-module-<n>`: it is the
        // page's n-th `<script>`, as an inline classic script is.
        if let Some((_, n)) = text.rsplit_once("#inline-module-") {
            if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) {
                return format!("inline#{n}");
            }
        }
        match Url::parse(text) {
            Ok(url) if matches!(url.scheme(), "http" | "https" | "ws" | "wss" | "file" | "data" | "blob") => {
                self.url(&url)
            }
            // `event:load`, `inline#2`: names, not URLs.
            _ => cap(text, MAX_SHORT_CHARS).0,
        }
    }

    /// Whether console messages keep their text (see
    /// [`InspectorConfig::keep_console_text`]).
    pub fn keeps_console_text(&self) -> bool {
        self.keep_console_text
    }

    /// A console call (`message` is capped here, and URLs in it scrubbed).
    pub fn console(&self, level: ConsoleLevel, message: &str, location: Option<SourceLocation>) {
        self.emit_with(|inspector| {
            let (message, truncated) = cap(message, MAX_MESSAGE_CHARS);
            let message = inspector.scrub_text(&message);
            InspectorEvent::Console { level, message, truncated, location }
        });
    }
}

fn round_ms(ms: f64) -> f64 {
    (ms * 1000.0).round() / 1000.0
}

/// `text` cut to at most `max` characters, and whether it was cut.
pub fn cap(text: &str, max: usize) -> (String, bool) {
    match text.char_indices().nth(max) {
        Some((end, _)) => (text[..end].to_string(), true),
        None => (text.to_string(), false),
    }
}

/// `url` with its credentials always removed, and its query and fragment
/// unless `keep_query`. A `data:` URL is reduced to `data:<mime>,…`; any
/// other URL that cannot have a host (`about:`, `blob:`) keeps its scheme
/// and, for `blob:`, its origin.
pub fn scrub_url(url: &Url, keep_query: bool) -> String {
    match url.scheme() {
        "data" => {
            let rest = &url.as_str()[5..];
            let mime = rest
                .split([',', ';'])
                .next()
                .unwrap_or("")
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || "/+-.".contains(*c))
                .take(64)
                .collect::<String>();
            format!("data:{mime},…")
        }
        "blob" => match Url::parse(url.path()) {
            Ok(inner) => format!("blob:{}", inner.origin().ascii_serialization()),
            Err(_) => "blob:".to_string(),
        },
        _ if url.cannot_be_a_base() => format!("{}:", url.scheme()),
        _ => {
            let mut clean = url.clone();
            let _ = clean.set_username("");
            let _ = clean.set_password(None);
            if !keep_query {
                clean.set_query(None);
                clean.set_fragment(None);
            }
            clean.to_string()
        }
    }
}

/// A script position as text gives it: path, line, column.
pub type PositionParts = (String, u32, u32);

/// What [`parse_error_text`] finds: name, message, where it was thrown, and
/// the stack lines.
pub type ErrorTextParts = (String, String, Option<PositionParts>, Option<String>);

/// An error string as the engine records it (`String(error)`, the JS
/// engine's `Name: message (path:line:col)` with its stack lines after it,
/// or what a listener's `report` captured) taken apart. Parses text only.
pub fn parse_error_text(text: &str) -> ErrorTextParts {
    let (head, stack) = match text.split_once('\n') {
        Some((head, rest)) => (head, Some(rest)),
        None => (text, None),
    };
    // Boa appends ` (path:line:col)` to the message line.
    let (head, head_position) = match head.rfind(" (") {
        Some(at) if head.ends_with(')') => match parse_position(&head[at + 2..head.len() - 1]) {
            Some(position) => (&head[..at], Some(position)),
            None => (head, None),
        },
        _ => (head, None),
    };
    let (name, message) = match head.split_once(": ") {
        Some((name, message))
            if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$') =>
        {
            (name.to_string(), message.to_string())
        }
        _ => (String::new(), head.to_string()),
    };
    let stack = stack
        .map(|s| s.trim_end_matches('\n'))
        .filter(|s| !s.trim().is_empty());
    // The innermost frame is where it was thrown.
    let position = stack
        .and_then(|s| s.lines().next())
        .and_then(|line| {
            // The path holds no space; its query may hold a `(`.
            let open = line.rfind(" (")?;
            parse_position(line[open + 2..].strip_suffix(')')?)
        })
        .or(head_position);
    (name, message, position, stack.map(str::to_string))
}

/// Where `scheme` next starts in `lower` from `from`: anywhere for a
/// `scheme://`, and for `data:` only where no letter or digit comes right
/// before it (`metadata:` is a word).
fn find_scheme(lower: &str, from: usize, scheme: &str) -> Option<usize> {
    let mut at = from;
    loop {
        let found = at + lower[at..].find(scheme)?;
        let word_before = lower[..found].chars().next_back().is_some_and(|c| c.is_ascii_alphanumeric());
        if scheme.ends_with("//") || !word_before {
            return Some(found);
        }
        at = found + scheme.len();
    }
}

/// What ends a URL's query in text and is not part of it: a frame's
/// `:line:col)`, else closing punctuation (`'`, `)`, `.`, ...).
fn query_tail(query: &str) -> &str {
    if let Some(inner) = query.strip_suffix(')') {
        let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
        if let Some((rest, column)) = inner.rsplit_once(':') {
            if let Some((_, line)) = rest.rsplit_once(':') {
                if digits(line) && digits(column) {
                    return &query[query.len() - line.len() - column.len() - 3..];
                }
            }
        }
    }
    let kept = query.len() - query.trim_end_matches(|c: char| ".,;:!?'\"`)]}>".contains(c)).len();
    &query[query.len() - kept..]
}

/// `path:line:col`, the path possibly holding colons itself.
fn parse_position(text: &str) -> Option<PositionParts> {
    let (rest, column) = text.rsplit_once(':')?;
    let (path, line) = rest.rsplit_once(':')?;
    let path = path.strip_prefix("unknown at ").unwrap_or(path);
    Some((path.trim().to_string(), line.parse().ok()?, column.parse().ok()?))
}

/// Writes each record as one JSON line.
pub struct JsonLinesSink<W: Write + Send> {
    out: W,
}

impl<W: Write + Send> JsonLinesSink<W> {
    pub fn new(out: W) -> Self {
        Self { out }
    }
}

impl<W: Write + Send> InspectorSink for JsonLinesSink<W> {
    fn record(&mut self, record: &InspectorRecord) {
        if let Ok(line) = serde_json::to_string(record) {
            let _ = writeln!(self.out, "{line}");
        }
    }

    fn flush(&mut self) {
        let _ = self.out.flush();
    }
}

/// JSON Lines into `path`, rotated when it would pass `max_bytes`: `path`
/// becomes `path.1`, `path.1` becomes `path.2`, ..., and `path.<keep>` is
/// deleted. Disk use stays under `max_bytes * (keep + 1)`. Write errors
/// are swallowed: diagnostics never fail a page.
pub struct RollingFileSink {
    path: PathBuf,
    max_bytes: u64,
    keep: usize,
    file: Option<std::io::BufWriter<std::fs::File>>,
    written: u64,
}

impl RollingFileSink {
    pub fn new(path: impl Into<PathBuf>, max_bytes: u64, keep: usize) -> std::io::Result<Self> {
        let path = path.into();
        let file = std::fs::OpenOptions::new().create(true).append(true).open(&path)?;
        let written = file.metadata().map(|m| m.len()).unwrap_or(0);
        Ok(Self {
            path,
            max_bytes: max_bytes.max(1),
            keep,
            file: Some(std::io::BufWriter::new(file)),
            written,
        })
    }

    fn rotated(path: &Path, n: usize) -> PathBuf {
        let mut name = path.as_os_str().to_owned();
        name.push(format!(".{n}"));
        PathBuf::from(name)
    }

    fn rotate(&mut self) {
        if let Some(mut file) = self.file.take() {
            let _ = file.flush();
        }
        if self.keep == 0 {
            let _ = std::fs::remove_file(&self.path);
        } else {
            let _ = std::fs::remove_file(Self::rotated(&self.path, self.keep));
            for n in (1..self.keep).rev() {
                let _ = std::fs::rename(Self::rotated(&self.path, n), Self::rotated(&self.path, n + 1));
            }
            let _ = std::fs::rename(&self.path, Self::rotated(&self.path, 1));
        }
        self.file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&self.path)
            .ok()
            .map(std::io::BufWriter::new);
        self.written = 0;
    }
}

impl InspectorSink for RollingFileSink {
    fn record(&mut self, record: &InspectorRecord) {
        let Ok(mut line) = serde_json::to_string(record) else { return };
        line.push('\n');
        if self.written > 0 && self.written + line.len() as u64 > self.max_bytes {
            self.rotate();
        }
        if let Some(file) = &mut self.file {
            if file.write_all(line.as_bytes()).is_ok() {
                self.written += line.len() as u64;
            }
        }
    }

    fn flush(&mut self) {
        if let Some(file) = &mut self.file {
            let _ = file.flush();
        }
    }
}

/// Collects records in memory (parity-capture's whole-load log, tests).
#[derive(Clone, Default)]
pub struct CollectingSink(pub Arc<Mutex<Vec<InspectorRecord>>>);

impl CollectingSink {
    pub fn records(&self) -> Vec<InspectorRecord> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl InspectorSink for CollectingSink {
    fn record(&mut self, record: &InspectorRecord) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).push(record.clone());
    }
}

/// Console calls by level.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsoleCounts {
    pub log: u64,
    pub info: u64,
    pub warn: u64,
    pub error: u64,
    pub debug: u64,
}

/// Counts over one load's records: what `parity-capture --event-summary`
/// prints and the site board publishes per site. Stable JSON: fields keep
/// their names and order, maps are sorted by key, and new fields are only
/// appended (`schema` goes up when one is). Documented in
/// `docs/INSPECTOR_EVENT_LOG.md`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct InspectorSummary {
    /// Version of this layout.
    pub schema: u32,
    /// Records the summary was made from.
    pub events: u64,
    pub console: ConsoleCounts,
    /// Uncaught exceptions, unhandled rejections included.
    pub script_errors: u64,
    /// Of `script_errors`, the unhandled rejections.
    pub unhandled_rejections: u64,
    /// `Interface.member` to how often page script reached it.
    pub unsupported_apis: std::collections::BTreeMap<String, u64>,
}

/// The summary layout version.
pub const SUMMARY_SCHEMA: u32 = 1;

impl InspectorSummary {
    /// Count `records`; the unsupported members' counts come from
    /// `unsupported` (the events only mark first sightings).
    pub fn from_records(records: &[InspectorRecord], unsupported: &[UnsupportedApiCount]) -> Self {
        let mut summary = InspectorSummary {
            schema: SUMMARY_SCHEMA,
            events: records.len() as u64,
            ..Default::default()
        };
        for record in records {
            match &record.event {
                InspectorEvent::Console { level, .. } => {
                    let counter = match level {
                        ConsoleLevel::Log => &mut summary.console.log,
                        ConsoleLevel::Info => &mut summary.console.info,
                        ConsoleLevel::Warn => &mut summary.console.warn,
                        ConsoleLevel::Error => &mut summary.console.error,
                        ConsoleLevel::Debug => &mut summary.console.debug,
                    };
                    *counter += 1;
                }
                InspectorEvent::ScriptError { unhandled_rejection, .. } => {
                    summary.script_errors += 1;
                    summary.unhandled_rejections += u64::from(*unhandled_rejection);
                }
                InspectorEvent::UnsupportedApi { interface, member, .. } => {
                    summary.unsupported_apis.entry(format!("{interface}.{member}")).or_insert(1);
                }
                _ => {}
            }
        }
        for api in unsupported {
            summary
                .unsupported_apis
                .insert(format!("{}.{}", api.interface, api.member), api.count);
        }
        summary
    }
}

/// What stage a network error failed at, as one word (no host, path or
/// other detail from the error's own text).
pub fn net_error_kind(error: &rustkit_net::NetError) -> &'static str {
    use rustkit_http::HttpError;
    use rustkit_net::NetError;
    match error {
        NetError::Blocked => "policy",
        NetError::Timeout(_) => "timeout",
        NetError::Cancelled => "cancelled",
        NetError::InvalidUrl(_) => "invalid_url",
        NetError::RequestFailed(_) | NetError::IoError(_) => "network",
        NetError::HttpError(http) => match http {
            HttpError::ConnectionFailed(text) => {
                let text = text.to_ascii_lowercase();
                if text.contains("no addresses") || text.contains("lookup") || text.contains("resolve")
                    || text.contains("name or service")
                {
                    "dns"
                } else {
                    "connect"
                }
            }
            HttpError::IoError(_) => "connect",
            HttpError::TlsError(_) => "tls",
            HttpError::Timeout => "timeout",
            HttpError::AddressDenied(_) => "policy",
            HttpError::TooManyRedirects => "redirect",
            HttpError::BodyTooLarge(_) => "over_budget",
            HttpError::InvalidResponse(_) => "http",
            HttpError::InvalidUrl(_) | HttpError::UnsupportedScheme(_) => "invalid_url",
        },
    }
}
