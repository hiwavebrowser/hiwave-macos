//! # RustKit JS
//!
//! JavaScript engine integration for the RustKit browser engine.
//!
//! ## Design Goals
//!
//! 1. **Engine abstraction**: Support multiple JS engines (Boa, V8)
//! 2. **Web API compatibility**: console, setTimeout, etc.
//! 3. **Safe interop**: Controlled boundary between Rust and JS
//! 4. **Async support**: Event loop integration

pub mod control;
mod import_map;
#[cfg(feature = "boa")]
mod module;
#[cfg(feature = "boa")]
pub use module::{FetchedModule, ModuleHandle, ModuleState};
pub use control::{ThreadStamp, ViewControl, Watchdog};
#[cfg(feature = "boa")]
mod executor;
#[cfg(feature = "boa")]
mod describe;
#[cfg(feature = "boa")]
mod rejections;
#[cfg(feature = "boa")]
mod diag;
#[cfg(feature = "boa")]
pub use diag::{standard_name, STANDARD_ERROR_NAMES};
#[cfg(all(test, feature = "boa"))]
mod module_tests;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use thiserror::Error;
use tracing::{debug, info, trace};

/// Errors that can occur in JS operations.
#[derive(Error, Debug)]
pub enum JsError {
    #[error("Execution error: {0}")]
    ExecutionError(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Type error: {0}")]
    TypeError(String),

    #[error("Engine not initialized")]
    NotInitialized,
}

/// Unique identifier for a timer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TimerId(u64);

impl TimerId {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::Relaxed))
    }

    pub fn raw(&self) -> u64 {
        self.0
    }
}

/// A JavaScript value.
#[derive(Debug, Clone)]
pub enum JsValue {
    Undefined,
    Null,
    Boolean(bool),
    Number(f64),
    String(String),
    Object,
    Array,
    Function,
}

impl JsValue {
    pub fn is_truthy(&self) -> bool {
        match self {
            JsValue::Undefined | JsValue::Null => false,
            JsValue::Boolean(b) => *b,
            JsValue::Number(n) => *n != 0.0 && !n.is_nan(),
            JsValue::String(s) => !s.is_empty(),
            _ => true,
        }
    }
}

/// Console log levels.
#[derive(Debug, Clone, Copy)]
pub enum LogLevel {
    Log,
    Info,
    Warn,
    Error,
    Debug,
}

/// Where in a script something happened: the path the script was evaluated
/// under ([`JsRuntime::evaluate_script_named`], a module's URL) and a
/// 1-based line and column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptPosition {
    pub path: String,
    pub line: u32,
    pub column: u32,
}

/// Console output handler: the level, the arguments formatted and joined,
/// and the calling script's position when the caller was in a named script.
pub type ConsoleHandler = Box<dyn Fn(LogLevel, &str, Option<&ScriptPosition>) + Send + Sync>;

/// A page error for diagnostics: an exception no script caught (in a
/// script, a module, a listener or a timer) or a promise rejected with no
/// handler. By default it holds no text the page chose (see `diag`): the
/// thrown value's type, a standard error name, a message's length, and the
/// positions of the page's frames.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorReport {
    /// What was thrown: `error`, `string`, `number`, `bigint`, `boolean`,
    /// `symbol`, `null`, `undefined`, `object`, `array` or `function`.
    pub value: &'static str,
    /// An error's `name` when it is a standard one, else `Error`; `None`
    /// for a value that is not an error.
    pub name: Option<&'static str>,
    /// The message text: only with error text kept
    /// ([`JsRuntime::set_error_reports`]). Never chosen by what the text
    /// says: an error that reached page script is the page's.
    pub message: Option<String>,
    /// The message's length in characters (an error's `message`, a thrown
    /// string).
    pub message_len: Option<usize>,
    /// The page's frames, innermost first: only positions in scripts the
    /// engine evaluated, never a function name.
    pub frames: Vec<ScriptPosition>,
    /// The first of `frames` is where it was thrown. Not so when the
    /// innermost page code is in no script of its own (`eval` code, an
    /// `on*` attribute's handler): it then has no position, and is never
    /// given its caller's.
    pub located: bool,
    /// A promise rejected with no handler.
    pub rejection: bool,
    /// A module that threw after a top-level `await` outlived the step that
    /// ran it: its path.
    pub module: Option<String>,
}

/// Timer callback.
pub type TimerCallback = Box<dyn FnOnce() + Send + 'static>;

/// A Rust function callable from script. It sees its arguments as plain
/// values (objects arrive as the payload-less `Object`/`Array`/`Function`),
/// and whatever it returns other than a primitive reaches script as
/// `undefined`. That keeps every Boa-managed value on the Boa side: a host
/// function can capture Rust state freely but can never hold a JS object.
pub type HostFunction = Box<dyn Fn(&[JsValue]) -> JsValue>;

/// Pending timer.
#[allow(dead_code)]
struct PendingTimer {
    callback: String, // JS code to execute
    delay: Duration,
    repeat: bool,
}

/// Default maximum number of loop iterations/pumps per `run_jobs` call.
pub const DEFAULT_MAX_JOB_ITERATIONS: u64 = 10_000;

/// JavaScript runtime configuration.
#[derive(Clone, Debug)]
pub struct JsRuntimeConfig {
    /// Enable strict mode.
    pub strict_mode: bool,
    /// Maximum execution time for script evaluation and job processing.
    pub timeout: Option<Duration>,
    /// Maximum number of job iterations per `run_jobs` call before halting runaway recursion.
    pub max_job_iterations: u64,
}

impl Default for JsRuntimeConfig {
    fn default() -> Self {
        Self {
            strict_mode: false,
            timeout: None,
            max_job_iterations: DEFAULT_MAX_JOB_ITERATIONS,
        }
    }
}

/// A wall-clock time after which the host stops answering script (see
/// [`JsRuntime::set_execution_deadline`]), and whether that has happened.
#[derive(Default)]
struct ExecutionDeadline {
    at: std::cell::Cell<Option<std::time::Instant>>,
    hit: std::cell::Cell<bool>,
    /// How long after `at` the first refused host call came.
    late: std::cell::Cell<Option<std::time::Duration>>,
    /// The view's interrupt word ([`JsRuntime::set_view_control`]), which
    /// another thread may set. Boa reads it itself, at every call
    /// (host calls included) and loop back-edge; this copy says whether it
    /// was what stopped the script.
    control: std::cell::RefCell<Option<Arc<control::ViewControl>>>,
    /// The turn running now ([`JsRuntime::begin_turn`]); 0 = none.
    turn: std::cell::Cell<u64>,
    /// Whether Boa compares the word with `turn` now: not during
    /// bookkeeping ([`JsRuntime::evaluate_bookkeeping`]).
    armed: std::cell::Cell<bool>,
    /// The stamp the open turn is on, for the grace turn after a stop.
    stamp: std::cell::RefCell<Option<Arc<control::ThreadStamp>>>,
    /// Whether this turn's grace turn has been opened.
    graced: std::cell::Cell<bool>,
}

impl ExecutionDeadline {
    /// Whether the view's interrupt word stops the running turn.
    fn interrupted(&self) -> bool {
        self.control
            .borrow()
            .as_ref()
            .is_some_and(|control| control.stops(self.turn.get()))
    }
}

/// JavaScript runtime that wraps the underlying engine.
pub struct JsRuntime {
    #[cfg(feature = "boa")]
    context: boa_engine::Context,
    /// The module host (see `module`): imports the graph asks for are
    /// recorded here for the embedder to fetch.
    #[cfg(feature = "boa")]
    modules: module::ModuleHost,
    #[cfg(feature = "boa")]
    executor: std::rc::Rc<executor::HostJobExecutor>,
    config: JsRuntimeConfig,
    console_handler: Option<Arc<ConsoleHandler>>,
    timers: Arc<Mutex<HashMap<TimerId, PendingTimer>>>,
    globals: HashMap<String, JsValue>,
    deadline: std::rc::Rc<ExecutionDeadline>,
    #[cfg(feature = "boa")]
    rejections: std::rc::Rc<rejections::RejectionHooks>,
    #[cfg(feature = "boa")]
    diag: std::rc::Rc<diag::Diagnostics>,
    /// The natives the embedder's own scripts are handed
    /// ([`evaluate_with_host`](Self::evaluate_with_host)); page script
    /// never sees this object.
    #[cfg(feature = "boa")]
    host: boa_engine::JsObject,
}

impl JsRuntime {
    /// Create a new JavaScript runtime.
    pub fn new() -> Result<Self, JsError> {
        Self::with_config(JsRuntimeConfig::default())
    }

    /// Create a new JavaScript runtime with configuration.
    pub fn with_config(config: JsRuntimeConfig) -> Result<Self, JsError> {
        info!("Initializing JavaScript runtime");

        #[cfg(feature = "boa")]
        let modules = module::ModuleHost::default();
        #[cfg(feature = "boa")]
        let executor = std::rc::Rc::new(executor::HostJobExecutor::with_limits(
            config.max_job_iterations,
            config.timeout,
        ));
        #[cfg(feature = "boa")]
        let rejections = std::rc::Rc::new(rejections::RejectionHooks::default());
        #[cfg(feature = "boa")]
        let mut context = boa_engine::Context::builder()
            .host_hooks(rejections.clone())
            .module_loader(modules.loader())
            .job_executor(executor.clone())
            .build()
            .map_err(|e| JsError::ExecutionError(e.to_string()))?;

        #[cfg(feature = "boa")]
        context.strict(config.strict_mode);
        #[cfg(feature = "boa")]
        let diag = std::rc::Rc::new(diag::Diagnostics::default());
        #[cfg(feature = "boa")]
        let host = host_object(&diag, &mut context);

        let mut runtime = Self {
            #[cfg(feature = "boa")]
            context,
            #[cfg(feature = "boa")]
            modules,
            #[cfg(feature = "boa")]
            executor,
            config,
            console_handler: None,
            timers: Arc::new(Mutex::new(HashMap::new())),
            globals: HashMap::new(),
            deadline: std::rc::Rc::default(),
            #[cfg(feature = "boa")]
            rejections,
            #[cfg(feature = "boa")]
            diag,
            #[cfg(feature = "boa")]
            host,
        };

        // Set up built-in APIs
        runtime.setup_console()?;
        #[cfg(feature = "boa")]
        {
            *runtime.diag.stack_getter.borrow_mut() = describe::stack_getter(&runtime.context);
        }

        debug!("JavaScript runtime initialized");
        Ok(runtime)
    }

    /// Get the runtime configuration.
    pub fn config(&self) -> &JsRuntimeConfig {
        &self.config
    }

    /// Set the microtask job iteration limit per `run_jobs` turn.
    pub fn set_max_job_iterations(&mut self, limit: u64) {
        self.config.max_job_iterations = limit;
        #[cfg(feature = "boa")]
        self.executor.set_max_job_iterations(limit);
    }

    /// Set the timeout for job execution.
    pub fn set_job_timeout(&mut self, timeout: Option<Duration>) {
        self.config.timeout = timeout;
        #[cfg(feature = "boa")]
        self.executor.set_timeout(timeout);
    }

    /// Run all currently pending jobs (microtasks and async completions).
    pub fn run_jobs(&mut self) -> Result<(), JsError> {
        #[cfg(feature = "boa")]
        {
            let result = self.context.run_jobs();
            self.consume_interrupt();
            result.map_err(|e| JsError::ExecutionError(e.to_string()))
        }
        #[cfg(not(feature = "boa"))]
        {
            Ok(())
        }
    }

    /// Set the console output handler. From now on each console call also
    /// notes where it was called from (one walk of the call stack per call).
    pub fn set_console_handler(&mut self, handler: ConsoleHandler) {
        self.console_handler = Some(Arc::new(handler));
        #[cfg(feature = "boa")]
        self.diag.console_capturing.set(true);
    }

    /// Hand the handler each console argument's type, and a string's length,
    /// instead of its text (see `describe::redacted_text`): for an embedder
    /// that records console calls where page text must not go. Off by
    /// default. The flag lives here, in Rust: no page script can change it.
    pub fn set_console_redaction(&mut self, on: bool) {
        #[cfg(feature = "boa")]
        self.diag.console_redacted.set(on);
        #[cfg(not(feature = "boa"))]
        let _ = on;
    }

    /// Describe page errors for [`take_error_reports`](Self::take_error_reports):
    /// uncaught exceptions, those the embedder's scripts report, and
    /// promises rejected with no handler. Off by default. `keep_text` keeps
    /// each message's text, which carries whatever the page put in it
    /// (cookies, form values, tokens): never for a log that will be shared.
    pub fn set_error_reports(&mut self, on: bool, keep_text: bool) {
        #[cfg(feature = "boa")]
        {
            self.diag.on.set(on);
            self.diag.keep_text.set(keep_text);
            self.rejections.tracking.set(on);
        }
        #[cfg(not(feature = "boa"))]
        let _ = (on, keep_text);
    }

    /// The page errors since the last call (with reports on), oldest first:
    /// exceptions, then modules that threw after a top-level `await`, then
    /// promises still rejected with no handler. Runs none of the page's
    /// code.
    pub fn take_error_reports(&mut self) -> Vec<ErrorReport> {
        #[cfg(feature = "boa")]
        {
            if !self.diag.on.get() {
                return Vec::new();
            }
            let mut reports = self.diag.take();
            for (module, reason) in self.take_late_module_rejections() {
                let mut report = self.diag.describe(&reason, None, false, &mut self.context);
                report.module = Some(module);
                reports.push(report);
            }
            let reasons = self.rejections.take();
            for reason in reasons {
                let report = self.diag.describe(&reason, None, true, &mut self.context);
                reports.push(report);
            }
            reports
        }
        #[cfg(not(feature = "boa"))]
        {
            Vec::new()
        }
    }

    /// What listeners and timers threw since the last call (reported by
    /// the embedder's scripts through the host natives), as text:
    /// `TypeError: x is not a function`, a thrown object as
    /// `[object Object]` (see `describe::text`). Held for at most 1024.
    pub fn take_reported_errors(&mut self) -> Vec<String> {
        #[cfg(feature = "boa")]
        {
            self.diag.take_reported_texts()
        }
        #[cfg(not(feature = "boa"))]
        {
            Vec::new()
        }
    }

    /// Unimplemented members the embedder's scripts reported page script
    /// reaching (`__rk.unsupported(interface, member)`) since the last call:
    /// `(interface, member, times, first caller's position)`. At most 64
    /// members are held.
    pub fn take_unsupported(&mut self) -> Vec<(String, String, u64, Option<ScriptPosition>)> {
        #[cfg(feature = "boa")]
        {
            self.diag.take_unsupported()
        }
        #[cfg(not(feature = "boa"))]
        {
            Vec::new()
        }
    }

    /// Evaluate one of the embedder's own scripts (never page script) as
    /// the body of a function whose parameter `__rk` is the host object:
    /// `__rk.reportError(e)` reports what a listener or timer threw,
    /// `__rk.unsupported(interface, member)` an unimplemented member. The
    /// object is reachable only from the closures such a script builds, so
    /// page script cannot report, forge or read through it.
    pub fn evaluate_with_host(&mut self, source: &str) -> Result<JsValue, JsError> {
        #[cfg(feature = "boa")]
        {
            use boa_engine::Source;
            let wrapped = format!("(function (__rk) {{\n{source}\n}})");
            let function = self
                .context
                .eval(Source::from_bytes(wrapped.as_bytes()).with_path(std::path::Path::new(diag::EMBEDDER_PATH)))
                .map_err(|e| JsError::ExecutionError(e.to_string()))?;
            let function = function
                .as_object()
                .filter(|f| f.is_callable())
                .ok_or_else(|| JsError::ExecutionError("host script is not a function".into()))?;
            let host = boa_engine::JsValue::from(self.host.clone());
            let result = function.call(&boa_engine::JsValue::undefined(), &[host], &mut self.context);
            let _ = self.context.run_jobs();
            self.flush_console_logs();
            match result {
                Ok(value) => Ok(self.convert_boa_value(&value)),
                Err(err) => Err(JsError::ExecutionError(describe::error_text(&err, &mut self.context))),
            }
        }
        #[cfg(not(feature = "boa"))]
        {
            let _ = source;
            Err(JsError::NotInitialized)
        }
    }

    /// Set up console API: `console.log`, `info`, `warn`, `error` and
    /// `debug` are native functions. Without a handler a call does nothing;
    /// with one its line is built at the call (no page code runs) and queued
    /// in Rust with the caller's position. Nothing on `console` or anywhere
    /// else page script can reach turns redaction off or adds to the queue.
    fn setup_console(&mut self) -> Result<(), JsError> {
        #[cfg(feature = "boa")]
        {
            use boa_engine::{js_string, object::ObjectInitializer, property::Attribute, NativeFunction};
            let mut console = ObjectInitializer::new(&mut self.context);
            for (name, level) in [
                ("log", LogLevel::Log),
                ("info", LogLevel::Info),
                ("warn", LogLevel::Warn),
                ("error", LogLevel::Error),
                ("debug", LogLevel::Debug),
            ] {
                let diag = self.diag.clone();
                // SAFETY: the closure holds an `Rc<Diagnostics>`, whose only
                // GC handle (the stack getter) stays rooted while it lives.
                let method = unsafe {
                    NativeFunction::from_closure(move |_this, args, context| {
                        diag.console_call(level, args, context);
                        Ok(boa_engine::JsValue::undefined())
                    })
                };
                console.function(method, js_string!(name), 0);
            }
            let console = console.build();
            self.context
                .register_global_property(js_string!("console"), console, Attribute::WRITABLE | Attribute::CONFIGURABLE)
                .map_err(|e| JsError::ExecutionError(e.to_string()))?;
        }
        Ok(())
    }

    /// Evaluate JavaScript code as one of the embedder's own scripts: its
    /// frames are passed over in error reports and console positions (the
    /// page's own scripts go through
    /// [`evaluate_script_named`](Self::evaluate_script_named)).
    pub fn evaluate_script(&mut self, source: &str) -> Result<JsValue, JsError> {
        self.evaluate(source, None)
    }

    /// Evaluate JavaScript code as the script `path`: error positions,
    /// stacks and console call sites name it. A `path` that does not parse
    /// as a URL (`inline#3`) leaves dynamic `import()` resolving against the
    /// document, as an unnamed script does.
    pub fn evaluate_script_named(&mut self, source: &str, path: &str) -> Result<JsValue, JsError> {
        self.evaluate(source, Some(path))
    }

    fn evaluate(&mut self, source: &str, path: Option<&str>) -> Result<JsValue, JsError> {
        trace!(len = source.len(), "Evaluating script");

        #[cfg(feature = "boa")]
        {
            use boa_engine::Source;

            let result = match path {
                Some(path) => {
                    self.diag.know_path(path);
                    self.context
                        .eval(Source::from_bytes(source).with_path(std::path::Path::new(path)))
                }
                None => self
                    .context
                    .eval(Source::from_bytes(source).with_path(std::path::Path::new(diag::EMBEDDER_PATH))),
            };
            self.consume_interrupt();
            // Promise reactions (`.then`, `await`) are jobs Boa queues but
            // does not run on its own; a page's async code never resumes
            // without this.
            let job_result = self.context.run_jobs();
            self.consume_interrupt();

            match result {
                Ok(value) => {
                    self.flush_console_logs();
                    if let Err(job_err) = job_result {
                        let msg = job_err.to_string();
                        // Only promote job queue limit or timeout breaches (runaway RangeError policy)
                        // to script failure; ordinary microtask rejections are handled by Promise rejection events
                        // and do not fail the synchronous script value that already completed successfully.
                        if msg.contains("Job queue") || msg.contains("limit") || msg.contains("timeout") {
                            if self.diag.on.get() {
                                let report = self.diag.describe_error(&job_err, &mut self.context);
                                self.diag.push(report);
                            }
                            return Err(JsError::ExecutionError(msg));
                        }
                    }
                    let js_value = self.convert_boa_value(&value);
                    Ok(js_value)
                }
                Err(err) => {
                    self.flush_console_logs();
                    if self.diag.on.get() {
                        let report = self.diag.describe_error(&err, &mut self.context);
                        self.diag.push(report);
                    }
                    let msg = describe::error_text(&err, &mut self.context);
                    Err(JsError::ExecutionError(msg))
                }
            }
        }

        #[cfg(not(feature = "boa"))]
        {
            let _ = path;
            Err(JsError::NotInitialized)
        }
    }

    /// Bound how long any one loop may run: past `max_iterations` it throws
    /// an error the script cannot catch. Page scripts are untrusted, and
    /// Boa has no wall-clock interrupt, so this is what stops a
    /// `while (true) {}` from hanging the engine.
    pub fn set_loop_iteration_limit(&mut self, max_iterations: u64) {
        #[cfg(feature = "boa")]
        self.context
            .runtime_limits_mut()
            .set_loop_iteration_limit(max_iterations);
        #[cfg(not(feature = "boa"))]
        let _ = max_iterations;
    }

    /// Stop script that is still running at `at`: from then on every host
    /// function fails, before it does anything, with an error the script
    /// cannot catch, so the whole call stack unwinds to whoever started
    /// it. `None` lifts it. Boa has no wall-clock interrupt of its own and
    /// the loop limit counts loop statements only; a script inside nested
    /// `forEach` callbacks is stopped by this and by nothing else. Script
    /// that never calls the host is not stopped.
    pub fn set_execution_deadline(&mut self, at: Option<std::time::Instant>) {
        self.deadline.at.set(at);
        self.deadline.hit.set(false);
        self.deadline.late.set(None);
    }

    /// Whether the deadline has stopped a host call since it was last set
    /// or asked about, or the view's interrupt word stops the running turn
    /// (which stays true until the turn ends).
    pub fn take_deadline_hit(&mut self) -> bool {
        self.deadline.hit.replace(false) | self.deadline.interrupted()
    }

    /// Give this runtime its view's control block: from now on another
    /// thread can stop the running turn by its interrupt word, at the next
    /// call (host calls included) or loop back-edge, the vendored Boa's
    /// patch. `None` takes it away.
    pub fn set_view_control(&mut self, control: Option<Arc<control::ViewControl>>) {
        #[cfg(feature = "boa")]
        self.context
            .set_interrupt_word(control.as_ref().map(|c| c.word().clone()));
        *self.deadline.control.borrow_mut() = control;
    }

    /// Open a turn on `stamp` for this runtime's view, with `budget` before
    /// the watchdog stops it, and return its id. Without a view control
    /// nothing is published and the id is 0. Every `begin_turn` is paired
    /// with an [`JsRuntime::end_turn`] on the same stamp.
    pub fn begin_turn(&mut self, stamp: &Arc<control::ThreadStamp>, budget: Duration) -> u64 {
        let Some(control) = self.deadline.control.borrow().clone() else {
            return 0;
        };
        let turn = stamp.begin_turn(&control, budget);
        *self.deadline.stamp.borrow_mut() = Some(stamp.clone());
        self.deadline.graced.set(false);
        self.set_turn(turn);
        turn
    }

    /// Close what [`JsRuntime::begin_turn`] opened.
    pub fn end_turn(&mut self, stamp: &control::ThreadStamp) {
        if self.deadline.control.borrow().is_none() {
            return;
        }
        stamp.end_turn();
        let open = stamp.current_turn();
        if open == 0 {
            *self.deadline.stamp.borrow_mut() = None;
        }
        self.set_turn(open);
    }

    /// The turn the interrupt word is compared with.
    fn set_turn(&mut self, turn: u64) {
        self.deadline.turn.set(turn);
        self.arm(turn != 0);
    }

    /// Whether Boa stops the running turn by the word (`ALL` it always does).
    fn arm(&mut self, armed: bool) {
        self.deadline.armed.set(armed);
        #[cfg(feature = "boa")]
        self.context
            .set_interrupt_turn(if armed { self.deadline.turn.get() } else { 0 });
    }

    /// Called each time script returns to Rust. The first time a turn's
    /// script has been stopped (by the word, or by the deadline's host-call
    /// refusal, which the watchdog's word then follows by a moment), the
    /// rest of the turn moves to a grace turn of
    /// [`STOP_GRACE`](control::STOP_GRACE): the stop has ended the script
    /// that ran too long, and what the turn's finished script queued (an
    /// XHR's `send` registers its request in a promise reaction) still runs,
    /// as it did before the word existed. The watchdog stops the grace turn
    /// too, and that stop is final. `take_deadline_hit` reports the stop.
    #[cfg(feature = "boa")]
    pub(crate) fn consume_interrupt(&mut self) {
        if !self.deadline.armed.get() || self.deadline.graced.get() {
            return;
        }
        if !(self.deadline.interrupted() || self.deadline.hit.get()) {
            return;
        }
        self.deadline.hit.set(true);
        self.deadline.graced.set(true);
        let control = self.deadline.control.borrow().clone();
        let stamp = self.deadline.stamp.borrow().clone();
        if let (Some(control), Some(stamp)) = (control, stamp) {
            stamp.end_turn();
            let grace = stamp.begin_turn(&control, control::STOP_GRACE);
            self.set_turn(grace);
        }
    }

    /// Evaluate the embedder's own bookkeeping script (reading a queue or a
    /// table the page's script filled), which the interrupt word never
    /// stops: a stop landing in it would lose what it reads, such as the
    /// requests a stopped turn's callbacks made. Only for short script that
    /// calls none of the page's functions. Runs no promise jobs (they are
    /// the page's, and run at the next evaluation); [`ALL`](control::ALL)
    /// still stops it.
    pub fn evaluate_bookkeeping(&mut self, source: &str) -> Result<JsValue, JsError> {
        #[cfg(feature = "boa")]
        {
            let armed = self.deadline.armed.get();
            self.arm(false);
            let result = self.context.eval(boa_engine::Source::from_bytes(source));
            self.arm(armed);
            match result {
                Ok(value) => Ok(self.convert_boa_value(&value)),
                Err(err) => Err(JsError::ExecutionError(err.to_string())),
            }
        }
        #[cfg(not(feature = "boa"))]
        {
            let _ = source;
            Err(JsError::NotInitialized)
        }
    }

    /// How long after the deadline the host call that stopped script came,
    /// once per stop. Script is only stopped where it calls the host, so
    /// this is the time it ran on past its budget.
    pub fn take_deadline_overrun(&mut self) -> Option<std::time::Duration> {
        self.deadline.late.take()
    }

    /// Hand the console calls made since the last flush to the handler
    /// now (module evaluation and job runs do not end in an
    /// `evaluate_script`, which flushes).
    pub fn flush_console(&mut self) {
        self.flush_console_logs();
    }

    /// Flush console logs and call handler.
    fn flush_console_logs(&mut self) {
        let Some(handler) = self.console_handler.clone() else {
            return;
        };

        #[cfg(feature = "boa")]
        {
            let logs = self.diag.take_console();
            for (level, msg, at) in logs {
                handler(level, &msg, at.as_ref());
            }
        }

        #[cfg(not(feature = "boa"))]
        {
            let _ = handler;
        }
    }

    /// Define a global function `name` that calls `function`.
    pub fn register_host_function(
        &mut self,
        name: &str,
        length: usize,
        function: HostFunction,
    ) -> Result<(), JsError> {
        #[cfg(feature = "boa")]
        {
            use boa_engine::{JsString, NativeFunction};

            // SAFETY: `HostFunction` only ever sees and returns the
            // crate's own `JsValue`, which holds no GC-managed data, so the
            // closure captures nothing the collector would need to trace.
            let deadline = self.deadline.clone();
            let native = unsafe {
                NativeFunction::from_closure(move |_this, args, _context| {
                    // No deadline (the default): the clock is not read.
                    let passed = deadline.at.get().and_then(|at| {
                        let now = std::time::Instant::now();
                        (now >= at).then(|| now - at)
                    });
                    if let Some(late) = passed {
                        // Logged here, at the first refusal, and not only
                        // by whoever started the script: if the stack then
                        // takes long to unwind, the log still says when.
                        if !deadline.hit.replace(true) {
                            deadline.late.set(Some(late));
                            tracing::warn!(
                                late_ms = late.as_millis() as u64,
                                "Script budget spent: host calls are refused until the script has unwound"
                            );
                        }
                        // Boa's runtime-limit errors are the ones script
                        // cannot catch, and it has none for time; `hit`
                        // says which limit this was.
                        return Err(boa_engine::error::RuntimeLimitError::LoopIteration.into());
                    }
                    let args: Vec<JsValue> = args.iter().map(from_boa_value).collect();
                    Ok(to_boa_value(function(&args)))
                })
            };
            self.context
                .register_global_callable(JsString::from(name), length, native)
                .map_err(|e| JsError::ExecutionError(e.to_string()))
        }

        #[cfg(not(feature = "boa"))]
        {
            let _ = (name, length, function);
            Err(JsError::NotInitialized)
        }
    }

    /// Convert Boa value to JsValue.
    #[cfg(feature = "boa")]
    fn convert_boa_value(&self, value: &boa_engine::JsValue) -> JsValue {
        from_boa_value(value)
    }

    /// Set a global variable.
    pub fn set_global(&mut self, name: &str, value: JsValue) -> Result<(), JsError> {
        self.globals.insert(name.to_string(), value.clone());

        // Set in the JS context
        let js_code = match value {
            JsValue::Undefined => format!("var {} = undefined;", name),
            JsValue::Null => format!("var {} = null;", name),
            JsValue::Boolean(b) => format!("var {} = {};", name, b),
            JsValue::Number(n) => format!("var {} = {};", name, n),
            JsValue::String(s) => format!("var {} = {:?};", name, s),
            _ => return Ok(()), // Complex types handled differently
        };

        self.evaluate_script(&js_code)?;
        Ok(())
    }

    /// Get a global variable.
    pub fn get_global(&mut self, name: &str) -> Result<JsValue, JsError> {
        self.evaluate_script(name)
    }

    /// Schedule a timeout (setTimeout equivalent).
    pub fn set_timeout(&mut self, code: &str, delay_ms: u32) -> TimerId {
        let id = TimerId::new();
        let timer = PendingTimer {
            callback: code.to_string(),
            delay: Duration::from_millis(delay_ms as u64),
            repeat: false,
        };

        self.timers.lock().unwrap().insert(id, timer);
        trace!(?id, delay_ms, "Timeout scheduled");
        id
    }

    /// Schedule an interval (setInterval equivalent).
    pub fn set_interval(&mut self, code: &str, interval_ms: u32) -> TimerId {
        let id = TimerId::new();
        let timer = PendingTimer {
            callback: code.to_string(),
            delay: Duration::from_millis(interval_ms as u64),
            repeat: true,
        };

        self.timers.lock().unwrap().insert(id, timer);
        trace!(?id, interval_ms, "Interval scheduled");
        id
    }

    /// Cancel a timeout or interval.
    pub fn clear_timer(&mut self, id: TimerId) {
        self.timers.lock().unwrap().remove(&id);
        trace!(?id, "Timer cleared");
    }

    /// Get pending timers that are due.
    pub fn get_due_timers(&self) -> Vec<(TimerId, String, bool)> {
        let timers = self.timers.lock().unwrap();
        timers
            .iter()
            .map(|(id, t)| (*id, t.callback.clone(), t.repeat))
            .collect()
    }

    /// Execute a timer callback.
    pub fn execute_timer(&mut self, id: TimerId) -> Result<(), JsError> {
        let timer = {
            let timers = self.timers.lock().unwrap();
            timers.get(&id).map(|t| (t.callback.clone(), t.repeat))
        };

        if let Some((callback, repeat)) = timer {
            self.evaluate_script(&callback)?;

            if !repeat {
                self.timers.lock().unwrap().remove(&id);
            }
        }

        Ok(())
    }

    /// Check if a global variable exists.
    pub fn has_global(&mut self, name: &str) -> bool {
        let check = format!("typeof {} !== 'undefined'", name);
        matches!(self.evaluate_script(&check), Ok(JsValue::Boolean(true)))
    }
}

#[cfg(feature = "boa")]
fn to_boa_value(value: JsValue) -> boa_engine::JsValue {
    use boa_engine::{JsString, JsValue as BoaValue};

    match value {
        JsValue::Null => BoaValue::null(),
        JsValue::Boolean(b) => BoaValue::from(b),
        JsValue::Number(n) => BoaValue::from(n),
        JsValue::String(s) => BoaValue::from(JsString::from(s.as_str())),
        JsValue::Undefined | JsValue::Object | JsValue::Array | JsValue::Function => {
            BoaValue::undefined()
        }
    }
}

/// The host object [`JsRuntime::evaluate_with_host`] hands the embedder's
/// scripts: natives that write into `diag`, built here, before any script.
#[cfg(feature = "boa")]
fn host_object(diag: &std::rc::Rc<diag::Diagnostics>, context: &mut boa_engine::Context) -> boa_engine::JsObject {
    use boa_engine::{js_string, object::ObjectInitializer, NativeFunction};
    let reporter = diag.clone();
    // SAFETY: the closure holds an `Rc<Diagnostics>`, whose only GC handles
    // (the stack getter, report values never) stay rooted while it lives.
    let report_error = unsafe {
        NativeFunction::from_closure(move |_this, args, context| {
            let value = args.first().cloned().unwrap_or_default();
            reporter.reported(&value, context);
            Ok(boa_engine::JsValue::undefined())
        })
    };
    let noter = diag.clone();
    // SAFETY: as above.
    let unsupported = unsafe {
        NativeFunction::from_closure(move |_this, args, context| {
            if let (Some(interface), Some(member)) = (
                args.first().and_then(|a| a.as_string()),
                args.get(1).and_then(|a| a.as_string()),
            ) {
                noter.unsupported(&interface.to_std_string_escaped(), &member.to_std_string_escaped(), context);
            }
            Ok(boa_engine::JsValue::undefined())
        })
    };
    ObjectInitializer::new(context)
        .function(report_error, js_string!("reportError"), 1)
        .function(unsupported, js_string!("unsupported"), 2)
        .build()
}

#[cfg(feature = "boa")]
fn from_boa_value(value: &boa_engine::JsValue) -> JsValue {
    if value.is_undefined() {
        JsValue::Undefined
    } else if value.is_null() {
        JsValue::Null
    } else if let Some(b) = value.as_boolean() {
        JsValue::Boolean(b)
    } else if let Some(n) = value.as_number() {
        JsValue::Number(n)
    } else if let Some(s) = value.as_string() {
        JsValue::String(s.to_std_string_escaped())
    } else if let Some(obj) = value.as_object() {
        if obj.is_array() {
            JsValue::Array
        } else if obj.is_callable() {
            JsValue::Function
        } else {
            JsValue::Object
        }
    } else {
        JsValue::Undefined
    }
}

impl Default for JsRuntime {
    fn default() -> Self {
        Self::new().expect("Failed to create default JsRuntime")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_evaluation() {
        let mut runtime = JsRuntime::new().unwrap();

        let result = runtime.evaluate_script("1 + 2").unwrap();
        assert!(matches!(result, JsValue::Number(n) if (n - 3.0).abs() < f64::EPSILON));
    }

    #[test]
    fn test_string_evaluation() {
        let mut runtime = JsRuntime::new().unwrap();

        let result = runtime.evaluate_script("'hello' + ' world'").unwrap();
        assert!(matches!(result, JsValue::String(s) if s == "hello world"));
    }

    #[test]
    fn test_boolean_evaluation() {
        let mut runtime = JsRuntime::new().unwrap();

        let result = runtime.evaluate_script("true && false").unwrap();
        assert!(matches!(result, JsValue::Boolean(false)));
    }

    #[test]
    fn test_global_variable() {
        let mut runtime = JsRuntime::new().unwrap();

        runtime
            .set_global("testVar", JsValue::Number(42.0))
            .unwrap();
        let result = runtime.evaluate_script("testVar * 2").unwrap();
        assert!(matches!(result, JsValue::Number(n) if (n - 84.0).abs() < f64::EPSILON));
    }

    #[test]
    fn test_console_exists() {
        let mut runtime = JsRuntime::new().unwrap();
        assert!(runtime.has_global("console"));
    }

    #[test]
    fn test_console_log() {
        let mut runtime = JsRuntime::new().unwrap();

        // Should not error
        runtime.evaluate_script("console.log('test')").unwrap();
    }

    #[test]
    fn test_timer_scheduling() {
        let mut runtime = JsRuntime::new().unwrap();

        let id1 = runtime.set_timeout("console.log('timeout')", 100);
        let id2 = runtime.set_interval("console.log('interval')", 50);

        assert_ne!(id1, id2);

        let timers = runtime.get_due_timers();
        assert_eq!(timers.len(), 2);

        runtime.clear_timer(id1);
        let timers = runtime.get_due_timers();
        assert_eq!(timers.len(), 1);
    }

    #[test]
    fn past_the_execution_deadline_a_host_call_unwinds_script_that_cannot_catch_it() {
        let mut runtime = JsRuntime::new().unwrap();
        let calls = std::rc::Rc::new(std::cell::Cell::new(0u32));
        let seen = calls.clone();
        runtime
            .register_host_function(
                "host",
                0,
                Box::new(move |_| {
                    seen.set(seen.get() + 1);
                    JsValue::Undefined
                }),
            )
            .unwrap();
        runtime
            .evaluate_script("var reached = [], caught = false;")
            .unwrap();
        let spin = "[1, 2, 3].forEach(function (n) { try { host(); } catch (e) { caught = true; } reached.push(n); });";

        // No deadline, and one that has not come: the host answers.
        runtime.evaluate_script(spin).unwrap();
        runtime.set_execution_deadline(Some(std::time::Instant::now() + Duration::from_secs(60)));
        runtime.evaluate_script(spin).unwrap();
        assert_eq!(calls.get(), 6);
        assert!(!runtime.take_deadline_hit());

        // Past it: the first host call ends the script, through the
        // `forEach` and the `try`, before the host function runs.
        runtime.set_execution_deadline(Some(std::time::Instant::now()));
        assert!(runtime.evaluate_script(spin).is_err());
        assert_eq!(calls.get(), 6);
        assert!(runtime.take_deadline_hit());
        assert!(!runtime.take_deadline_hit());

        // The stop comes where script next calls the host, and says how
        // late that was: this script computes for a while first.
        runtime.set_execution_deadline(Some(std::time::Instant::now()));
        assert_eq!(runtime.take_deadline_overrun(), None);
        let busy = "var t = Date.now(); [1].forEach(function () { while (Date.now() - t < 30) {} host(); });";
        assert!(runtime.evaluate_script(busy).is_err());
        let late = runtime.take_deadline_overrun().expect("the stop reports how late it came");
        // `Date.now()` counts whole milliseconds, so the 30 ms loop is a
        // little under 30 ms of wall clock.
        assert!(late >= Duration::from_millis(20), "{late:?}");
        assert_eq!(runtime.take_deadline_overrun(), None);
        assert!(runtime.take_deadline_hit());

        // Lifted: the runtime runs script again, and nothing was caught.
        runtime.set_execution_deadline(None);
        let result = runtime.evaluate_script("host(); caught === false && reached.length === 6").unwrap();
        assert!(matches!(result, JsValue::Boolean(true)), "{result:?}");
        assert_eq!(calls.get(), 7);
    }

    #[test]
    fn test_function_execution() {
        let mut runtime = JsRuntime::new().unwrap();

        runtime
            .evaluate_script("function add(a, b) { return a + b; }")
            .unwrap();
        let result = runtime.evaluate_script("add(2, 3)").unwrap();
        assert!(matches!(result, JsValue::Number(n) if (n - 5.0).abs() < f64::EPSILON));
    }

    #[test]
    fn test_object_creation() {
        let mut runtime = JsRuntime::new().unwrap();

        let result = runtime.evaluate_script("({ name: 'test' })").unwrap();
        assert!(matches!(result, JsValue::Object));
    }

    #[test]
    fn test_array_creation() {
        let mut runtime = JsRuntime::new().unwrap();

        let result = runtime.evaluate_script("[1, 2, 3]").unwrap();
        assert!(matches!(result, JsValue::Array));
    }

    #[test]
    fn a_runaway_loop_throws_instead_of_hanging() {
        let mut runtime = JsRuntime::new().unwrap();
        runtime.set_loop_iteration_limit(10_000);
        let result = runtime.evaluate_script("try { while (true) {} } catch (e) {} 'caught'");
        assert!(result.is_err(), "the limit must not be catchable: {result:?}");
        // The runtime is still usable afterwards.
        let after = runtime.evaluate_script("1 + 1").unwrap();
        assert!(matches!(after, JsValue::Number(n) if n == 2.0));
    }

    #[test]
    fn promise_reactions_run() {
        let mut runtime = JsRuntime::new().unwrap();
        runtime
            .evaluate_script("var done = false; Promise.resolve().then(function() { done = true; });")
            .unwrap();
        let done = runtime.evaluate_script("done").unwrap();
        assert!(matches!(done, JsValue::Boolean(true)));
    }

    #[test]
    fn a_runaway_microtask_chain_throws_instead_of_hanging() {
        let mut runtime = JsRuntime::new().unwrap();
        runtime.set_max_job_iterations(500);
        let result = runtime.evaluate_script("function again() { Promise.resolve().then(again); } again();");
        assert!(result.is_err(), "runaway microtask chain must error instead of hanging: {result:?}");
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Job queue") || err_msg.contains("limit"), "error must mention limit: {err_msg}");
        // The runtime is still usable afterwards.
        let after = runtime.evaluate_script("1 + 1").unwrap();
        assert!(matches!(after, JsValue::Number(n) if n == 2.0));
    }

    #[test]
    fn strict_mode_in_config_is_respected() {
        let mut strict_rt = JsRuntime::with_config(JsRuntimeConfig {
            strict_mode: true,
            ..Default::default()
        }).unwrap();
        let result = strict_rt.evaluate_script("undeclaredVar = 42;");
        assert!(result.is_err(), "assignment to undeclared variable must fail in strict mode");

        let mut non_strict_rt = JsRuntime::with_config(JsRuntimeConfig {
            strict_mode: false,
            ..Default::default()
        }).unwrap();
        let result2 = non_strict_rt.evaluate_script("undeclaredVar = 42;");
        assert!(result2.is_ok(), "assignment to undeclared variable succeeds in non-strict mode");
    }

    #[test]
    fn job_timeout_in_config_is_respected() {
        let mut runtime = JsRuntime::with_config(JsRuntimeConfig {
            timeout: Some(Duration::from_millis(20)),
            max_job_iterations: 1_000_000,
            ..Default::default()
        }).unwrap();
        // Infinite recursion with timeout
        let result = runtime.evaluate_script("function loop() { Promise.resolve().then(loop); } loop();");
        assert!(result.is_err(), "must timeout rather than hanging");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("timeout") || err.contains("limit"), "must report timeout or limit: {err}");
        // Runtime remains usable
        let after = runtime.evaluate_script("40 + 2").unwrap();
        assert!(matches!(after, JsValue::Number(n) if n == 42.0));
    }

    #[test]
    fn host_functions_take_and_return_primitives() {
        let mut runtime = JsRuntime::new().unwrap();
        runtime
            .register_host_function(
                "__host_echo",
                2,
                Box::new(|args| match args {
                    [JsValue::String(s), JsValue::Number(n)] => {
                        JsValue::String(format!("{s}:{n}"))
                    }
                    [JsValue::Object] => JsValue::Object,
                    _ => JsValue::Null,
                }),
            )
            .unwrap();
        let echoed = runtime.evaluate_script("__host_echo('a', 2)").unwrap();
        assert!(matches!(echoed, JsValue::String(s) if s == "a:2"));
        let none = runtime.evaluate_script("__host_echo()").unwrap();
        assert!(matches!(none, JsValue::Null));
        // A non-primitive return reaches script as undefined.
        let object = runtime.evaluate_script("typeof __host_echo({})").unwrap();
        assert!(matches!(object, JsValue::String(s) if s == "undefined"));
    }

    #[test]
    fn test_error_handling() {
        let mut runtime = JsRuntime::new().unwrap();

        let result = runtime.evaluate_script("nonexistent.property");
        assert!(result.is_err());
    }

    #[test]
    fn console_handler_receives_flushed_logs_without_recursion() {
        let mut runtime = JsRuntime::new().unwrap();
        let received = Arc::new(Mutex::new(Vec::new()));
        let recv_clone = received.clone();
        runtime.set_console_handler(Box::new(move |level, msg, _at| {
            recv_clone.lock().unwrap().push((format!("{level:?}"), msg.to_string()));
        }));

        runtime.evaluate_script("console.log('hello', 'world'); console.warn('caution');").unwrap();

        let logs = received.lock().unwrap().clone();
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0], ("Log".to_string(), "hello world".to_string()));
        assert_eq!(logs[1], ("Warn".to_string(), "caution".to_string()));

        // Subsequent script evaluation only delivers new logs (buffer was flushed)
        runtime.evaluate_script("console.error('oops');").unwrap();
        let logs2 = received.lock().unwrap().clone();
        assert_eq!(logs2.len(), 3);
        assert_eq!(logs2[2], ("Error".to_string(), "oops".to_string()));
    }

    #[test]
    fn console_calls_carry_the_named_scripts_position() {
        let mut runtime = JsRuntime::new().unwrap();
        let received = Arc::new(Mutex::new(Vec::new()));
        let recv_clone = received.clone();
        runtime.set_console_handler(Box::new(move |_level, msg, at| {
            recv_clone.lock().unwrap().push((msg.to_string(), at.cloned()));
        }));
        runtime
            .evaluate_script_named("var a = 1;\nfunction f() {\n  console.log('in f');\n}\nf();", "inline#4")
            .unwrap();
        // Unnamed: no position.
        runtime.evaluate_script("console.log('anonymous')").unwrap();
        let logs = received.lock().unwrap().clone();
        assert_eq!(
            logs,
            vec![
                (
                    "in f".to_string(),
                    Some(ScriptPosition { path: "inline#4".into(), line: 3, column: 14 })
                ),
                ("anonymous".to_string(), None),
            ]
        );
    }

    #[test]
    fn a_named_script_names_itself_in_its_errors() {
        let mut runtime = JsRuntime::new().unwrap();
        let err = runtime.evaluate_script_named("1;\n  null.x;", "inline#2").unwrap_err();
        assert!(err.to_string().contains("(inline#2:2:"), "{err}");
    }

    /// A thrown value that is not an error is described (`[object Object]`,
    /// the string itself), never pretty-printed with its properties, and
    /// keeps its frames; an error-like object (a prototype of
    /// `Error.prototype`) keeps its name and message.
    #[test]
    fn a_thrown_non_error_is_described_not_printed() {
        let mut runtime = JsRuntime::new().unwrap();
        let err = runtime
            .evaluate_script_named("1;\nthrow {user: 'a', token: 'SECRET'};", "inline#1")
            .unwrap_err()
            .to_string();
        assert!(err.contains("[object Object]\n    at <main> (inline#1:2:"), "{err}");
        assert!(!err.contains("SECRET"), "{err}");
        let err = runtime.evaluate_script_named("throw 'plain';", "inline#2").unwrap_err().to_string();
        assert!(err.contains("plain\n    at <main> (inline#2:1:"), "{err}");
        let err = runtime
            .evaluate_script_named(
                "function D(m, n) { this.message = m; this.name = n; }\nD.prototype = Object.create(Error.prototype);\nthrow new D('no', 'NotSupportedError');",
                "inline#3",
            )
            .unwrap_err()
            .to_string();
        assert!(err.contains("NotSupportedError: no\n    at <main> (inline#3:3:"), "{err}");
    }

    #[test]
    fn unhandled_rejections_are_reported_only_when_tracked_and_unhandled() {
        let mut runtime = JsRuntime::new().unwrap();
        runtime.evaluate_script("Promise.reject(new Error('untracked'));").unwrap();
        assert!(runtime.take_error_reports().is_empty());

        runtime.set_error_reports(true, false);
        runtime
            .evaluate_script_named(
                "Promise.reject(new TypeError('lost'));\nPromise.reject(1).catch(function () {});\nvar late = Promise.reject('late'); late.then(null, function () {});",
                "inline#1",
            )
            .unwrap();
        let rejections = runtime.take_error_reports();
        assert_eq!(rejections.len(), 1, "{rejections:?}");
        let r = &rejections[0];
        assert_eq!((r.value, r.name, r.message.as_deref(), r.message_len, r.rejection), ("error", Some("TypeError"), None, Some(4), true));
        assert_eq!(r.frames, vec![ScriptPosition { path: "inline#1".into(), line: 1, column: 16 }]);
        assert!(runtime.take_error_reports().is_empty(), "taken once");
    }

    #[test]
    fn console_flush_handles_throwing_tostring_gracefully() {
        let mut runtime = JsRuntime::new().unwrap();
        let received = Arc::new(Mutex::new(Vec::new()));
        let recv_clone = received.clone();
        runtime.set_console_handler(Box::new(move |_level, msg, _at| {
            recv_clone.lock().unwrap().push(msg.to_string());
        }));

        runtime
            .evaluate_script("console.log('val:', { toString() { throw new Error('boom'); } });")
            .unwrap();

        // The page's `toString` is never called: an object is its tag.
        let logs = received.lock().unwrap().clone();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0], "val: [object Object]");
    }

    /// The line is built at the call, from the values as they are then, and
    /// none of the page's code runs: not `toString`, not a getter, not a
    /// Proxy trap, neither at the call nor when the engine drains the log.
    #[test]
    fn console_lines_run_no_page_code_and_are_built_at_the_call() {
        let mut runtime = JsRuntime::new().unwrap();
        let received = Arc::new(Mutex::new(Vec::new()));
        let recv_clone = received.clone();
        runtime.set_console_handler(Box::new(move |_level, msg, _at| {
            recv_clone.lock().unwrap().push(msg.to_string());
        }));
        runtime
            .evaluate_script(
                "var n = 0;
                 var o = { toString() { n += 100; return 'side'; } };
                 var p = new Proxy({}, { get() { n += 1; throw new Error('trap'); } });
                 var e = new TypeError('bad thing');
                 var arr = [1, 2];
                 console.log(o, p, e, arr, Symbol('s'), 12, null, undefined, 'text', 10n);
                 arr.push(3);",
            )
            .unwrap();
        assert_eq!(
            received.lock().unwrap().clone(),
            vec!["[object Object] [object Object] TypeError: bad thing [object Array] Symbol(s) 12 null undefined text 10"]
        );
        assert_eq!(runtime.evaluate_script("n").unwrap().is_truthy(), false, "page code ran");
    }

    /// Nothing page script can reach turns redaction off or writes a line
    /// of its own: the console methods are natives, the flag and the queue
    /// live in Rust, and no prototype the page can patch is used.
    #[test]
    fn page_script_cannot_switch_redaction_off_or_forge_console_lines() {
        let mut runtime = JsRuntime::new().unwrap();
        let received = Arc::new(Mutex::new(Vec::new()));
        let recv_clone = received.clone();
        runtime.set_console_handler(Box::new(move |_level, msg, _at| {
            recv_clone.lock().unwrap().push(msg.to_string());
        }));
        runtime.set_console_redaction(true);
        runtime
            .evaluate_script(
                "var secret = 'SECRET';
                 try { console.__rustkit_capture(false); } catch (e) {}
                 try { console._logs.push({level: 'error', msg: 'FORGED'}); } catch (e) {}
                 console._flush = function () { return [{level: 'error', msg: 'FORGED'}]; };
                 __rustkit_console_format = function () { return 'FORGED'; };
                 Function.prototype.apply = function () { return 'FORGED'; };
                 Array.prototype.push = function () { return 0; };
                 Array.from = function () { return ['FORGED']; };
                 JSON.stringify = function () { return 'FORGED'; };
                 String = function () { return 'FORGED'; };
                 Object.prototype[Symbol.toPrimitive] = function () { return 'FORGED'; };
                 Object.defineProperty(Object.prototype, 'msg', { get: function () { return 'FORGED'; } });
                 console.log(secret, 1);",
            )
            .unwrap();
        assert_eq!(received.lock().unwrap().clone(), vec!["<string len=6> <number>"]);
    }

    /// A page that logs without end holds at most 1024 lines between flushes.
    #[test]
    fn console_lines_waiting_for_a_flush_are_bounded() {
        let mut runtime = JsRuntime::new().unwrap();
        let received = Arc::new(Mutex::new(Vec::new()));
        let recv_clone = received.clone();
        runtime.set_console_handler(Box::new(move |_level, msg, _at| {
            recv_clone.lock().unwrap().push(msg.to_string());
        }));
        let module = runtime.begin_module("https://a.test/m.js", "for (let i = 0; i < 5000; i++) console.log(i);");
        assert!(module.is_ok());
        let _ = runtime.poll_module(&module.unwrap());
        runtime.flush_console();
        let lines = received.lock().unwrap().clone();
        assert_eq!(lines.len(), 1024);
        assert_eq!(lines.last().map(String::as_str), Some("4999"));
    }

    /// Code with no named script of its own (`eval`) has no position; the
    /// caller's position is not passed off as its.
    #[test]
    fn a_console_call_from_eval_code_has_no_position() {
        let mut runtime = JsRuntime::new().unwrap();
        let received = Arc::new(Mutex::new(Vec::new()));
        let recv_clone = received.clone();
        runtime.set_console_handler(Box::new(move |_level, msg, at| {
            recv_clone.lock().unwrap().push((msg.to_string(), at.cloned()));
        }));
        runtime.evaluate_script_named("1;\neval(\"console.log('in eval')\");", "inline#1").unwrap();
        assert_eq!(received.lock().unwrap().clone(), vec![("in eval".to_string(), None)]);
    }

    /// A rejection's reason is described without its `toString` or a
    /// `stack` getter of the page's.
    #[test]
    fn describing_a_rejection_runs_no_page_code() {
        let mut runtime = JsRuntime::new().unwrap();
        runtime.set_error_reports(true, true);
        runtime
            .evaluate_script_named(
                "var n = 0;
                 Promise.reject({ toString() { n += 1; return 'x'; } });
                 var e = new Error('plain');
                 Object.defineProperty(e, 'stack', { get() { n += 10; return 'fake'; } });
                 Object.defineProperty(e, 'name', { get() { n += 100; return 'Fake'; } });
                 Promise.reject(e);",
                "inline#1",
            )
            .unwrap();
        let rejections = runtime.take_error_reports();
        assert_eq!(rejections.len(), 2);
        assert_eq!((rejections[0].value, rejections[0].message.as_deref()), ("object", None));
        assert_eq!((rejections[1].name, rejections[1].message.as_deref()), (Some("Error"), Some("plain")));
        assert_eq!(rejections[1].frames.first().map(|f| f.line), Some(3), "the stack is where the error was made: {rejections:?}");
        assert_eq!(runtime.evaluate_script("n").unwrap().is_truthy(), false, "page code ran");
    }

    /// One function declared in `eval` code, which throws after a console
    /// call, called from the page's script `inline#1`: the error's frames
    /// and whether it is located, and the console call's position.
    fn eval_declared_throw(declare: &str, call: &str) -> (Vec<ScriptPosition>, bool, Option<ScriptPosition>) {
        let mut runtime = JsRuntime::new().unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = seen.clone();
        runtime.set_console_handler(Box::new(move |_level, _msg, at| sink.lock().unwrap().push(at.cloned())));
        runtime.set_error_reports(true, false);
        let body = "{ console.log(\"c\"); throw new Error(\"x\"); }";
        let script = format!("1;\neval('{}');\n{call};", declare.replace("BODY", body));
        assert!(runtime.evaluate_script_named(&script, "inline#1").is_err());
        let reports = runtime.take_error_reports();
        assert_eq!(reports.len(), 1, "{script}: {reports:?}");
        let console = seen.lock().unwrap().clone();
        assert_eq!(console.len(), 1, "{script}");
        (reports[0].frames.clone(), reports[0].located, console[0].clone())
    }

    /// A function's name is the page's (a declaration, a literal or
    /// computed key, `Object.defineProperty(f, 'name')`): whether its error
    /// and console call have a place, and which, never depends on it. A
    /// function `eval` code declared has no script of its own whatever it
    /// is called, so neither is given its caller's position.
    #[test]
    fn a_page_chosen_function_name_never_decides_a_place() {
        let ordinary = eval_declared_throw("function gg() BODY", "gg()");
        assert_eq!(ordinary, (vec![ScriptPosition { path: "inline#1".into(), line: 3, column: 3 }], false, None));
        for (declare, call) in [
            ("function anonymous() BODY", "anonymous()"),
            ("var o = { \"f eval at g\": function () BODY }", "o[\"f eval at g\"]()"),
            ("var o = { \"<eval>\": function () BODY }", "o[\"<eval>\"]()"),
            ("var o = { [\"<e\" + \"val>\"]: function () BODY }", "o[\"<eval>\"]()"),
            ("var o = { [\"anony\" + \"mous\"]: function () BODY }", "o.anonymous()"),
            ("function h() BODY Object.defineProperty(h, \"name\", { value: \"<eval>\" })", "h()"),
            ("function h() BODY Object.defineProperty(h, \"name\", { value: \"anonymous\" })", "h()"),
        ] {
            let (frames, located, console) = eval_declared_throw(declare, call);
            assert_eq!((frames.len(), located, console), (1, false, None), "{declare}");
            assert_eq!((frames[0].path.as_str(), frames[0].line), ("inline#1", 3), "{declare}");
        }
    }

    /// The same names on functions a page script declared: located where
    /// they throw, like any other.
    #[test]
    fn a_page_function_named_like_engine_code_is_located_where_it_throws() {
        for (declare, call) in [
            ("function gg() BODY", "gg()"),
            ("function anonymous() BODY", "anonymous()"),
            ("var o = { \"f eval at g\": function () BODY }", "o[\"f eval at g\"]()"),
            ("var o = { \"<eval>\": function () BODY }", "o[\"<eval>\"]()"),
            ("function h() BODY Object.defineProperty(h, \"name\", { value: \"<eval>\" })", "h()"),
        ] {
            let mut runtime = JsRuntime::new().unwrap();
            let seen = Arc::new(Mutex::new(Vec::new()));
            let sink = seen.clone();
            runtime.set_console_handler(Box::new(move |_level, _msg, at| sink.lock().unwrap().push(at.cloned())));
            runtime.set_error_reports(true, false);
            let body = "{\nconsole.log('c');\n  throw new Error('x'); }";
            let script = format!("{};\n{call};", declare.replace("BODY", body));
            assert!(runtime.evaluate_script_named(&script, "inline#1").is_err());
            let reports = runtime.take_error_reports();
            assert!(reports[0].located, "{declare}: {reports:?}");
            assert_eq!(reports[0].frames[0], ScriptPosition { path: "inline#1".into(), line: 3, column: 9 }, "{declare}");
            let console = seen.lock().unwrap().clone();
            assert_eq!(console, vec![Some(ScriptPosition { path: "inline#1".into(), line: 2, column: 12 })], "{declare}");
        }
    }

    /// `eval` code and a function built by `new Function` are in no script
    /// of their own: no place, never their caller's.
    #[test]
    fn eval_and_new_function_code_have_no_place() {
        for script in ["1;\neval('console.log(1); null.q');", "1;\nvar f = new Function('console.log(1); null.q');\nf();"] {
            let mut runtime = JsRuntime::new().unwrap();
            let seen = Arc::new(Mutex::new(Vec::new()));
            let sink = seen.clone();
            runtime.set_console_handler(Box::new(move |_level, _msg, at| sink.lock().unwrap().push(at.cloned())));
            runtime.set_error_reports(true, false);
            assert!(runtime.evaluate_script_named(script, "inline#1").is_err());
            let reports = runtime.take_error_reports();
            assert_eq!(reports.len(), 1, "{script}");
            assert!(!reports[0].located, "{script}: {reports:?}");
            assert_eq!(reports[0].frames.first().map(|f| f.line), Some(2 + script.matches("new Function").count() as u32), "{script}");
            assert_eq!(seen.lock().unwrap().clone(), vec![None], "{script}");
        }
    }

    /// The embedder's own scripts are passed over by where they are, not
    /// by what their functions are called: an error a function of theirs
    /// throws is placed at the page's call, whatever the function's name.
    #[test]
    fn the_embedders_own_frames_are_passed_over_whatever_their_name() {
        for name in ["glueThrow", "anonymous"] {
            let mut runtime = JsRuntime::new().unwrap();
            runtime
                .evaluate_with_host(&format!("globalThis.{name}Fn = function {name}() {{ throw new TypeError('g'); }};"))
                .unwrap();
            runtime.evaluate_script("globalThis.pathlessFn = function anonymous() { null.q; };").unwrap();
            runtime.set_error_reports(true, false);
            for callee in [format!("{name}Fn"), "pathlessFn".to_string()] {
                assert!(runtime.evaluate_script_named(&format!("1;\n  {callee}();"), "inline#1").is_err());
                let reports = runtime.take_error_reports();
                assert!(reports[0].located, "{name} {callee}: {reports:?}");
                assert_eq!((reports[0].frames[0].line, reports[0].frames[0].column), (2, 3 + callee.len() as u32), "{callee}");
            }
        }
    }

    /// An error's message is text page script can choose (a property key in
    /// a native error's message): a position or frame line in it is never
    /// taken for the engine's.
    #[test]
    fn a_position_in_an_errors_message_is_never_its_place() {
        use boa_engine::{JsError as BoaError, JsNativeError};
        let mut runtime = JsRuntime::new().unwrap();
        runtime.set_error_reports(true, false);
        runtime.evaluate_script_named("1;", "inline#1").unwrap();
        for message in ["cannot redefine property: k (inline#1:9:9)", "k\n    at q (inline#1:9:9)", "k\n    at q (inline#1:9:9)\nz"] {
            let error = BoaError::from(JsNativeError::typ().with_message(message));
            let report = runtime.diag.describe_error(&error, &mut runtime.context);
            assert_eq!((report.frames, report.located), (Vec::new(), false), "{message}");
        }
    }
}
