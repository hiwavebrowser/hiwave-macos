//! The module host (Z phase, package C0): Boa's `ModuleLoader` wired to a
//! document.
//!
//! Boa asks its loader for each import and expects the answer through a
//! callback it may be handed later. This host uses that to stay out of the
//! network entirely: an import the graph asks for is **recorded**, not
//! fetched. The embedder (the engine) takes the recorded URLs, fetches them
//! under its own policy, and supplies each source back; Boa then carries on
//! loading the graph. Nothing here blocks, nothing here opens a connection.
//!
//! Identity is the URL. The module map is keyed by the URL a module was
//! *finally* served from, so two roots (or a redirect) that reach the same
//! file share one module and it is evaluated once. A specifier is resolved
//! against the importing module's URL; a bare specifier (`react`) is refused
//! as in a browser with no import map.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::Path;
use std::rc::Rc;

use boa_engine::builtins::promise::PromiseState;
use boa_engine::module::{ModuleLoader, ModuleRequest, Referrer};
use boa_engine::object::builtins::JsPromise;
use boa_engine::{js_string, Context, JsNativeError, JsObject, JsResult, JsValue, Module, Source};
use url::Url;

use crate::JsRuntime;

struct Waiter {
    url: String,
    specifier: String,
    /// This wait came from a dynamic `import()`, not a static dependency.
    dynamic: bool,
    sender: tokio::sync::oneshot::Sender<JsResult<Module>>,
}

/// Boa's `ModuleLoader` for a document. Interior-mutable and `Rc`-shared:
/// the context owns one reference, the runtime the other.
#[derive(Default)]
pub(crate) struct HostModuleLoader {
    /// The document's URL, the base for a root module's own imports when it
    /// has no path of its own.
    base: RefCell<Option<Url>>,
    /// Every module parsed so far, by the URL it was served from.
    modules: RefCell<HashMap<String, Module>>,
    /// Imports waiting for their source.
    waiting: RefCell<Vec<Waiter>>,
    /// URLs the graph asked for that the host has not been told about yet.
    /// `dynamic` is an `import()`, which does not keep a load promise pending.
    requested: RefCell<Vec<RequestedModule>>,
    /// URLs ever handed to the host (a URL is asked for once).
    asked: RefCell<HashSet<String>>,
    /// The page's import maps, merged.
    import_map: RefCell<crate::import_map::ImportMap>,
}

impl HostModuleLoader {
    fn resolve(&self, specifier: &str, referrer: Option<&str>) -> Result<Url, String> {
        let document = self.base.borrow().clone();
        let referrer_url = referrer
            .and_then(|r| Url::parse(r).ok())
            .or_else(|| document.clone());
        let Some(referrer_url) = referrer_url else {
            // No base at all: only an absolute URL can be resolved.
            return Url::parse(specifier).map_err(|_| {
                format!("Failed to resolve module specifier '{specifier}': no base URL")
            });
        };
        // A relative specifier resolves against the importing module; the
        // import map (if any) decides everything else.
        self.import_map
            .borrow()
            .resolve(specifier, &referrer_url, &referrer_url)
    }

    pub fn register_module(&self, specifier: &str, module: Module) {
        self.modules
            .borrow_mut()
            .insert(specifier.to_string(), module);
    }

    pub fn get_module(&self, specifier: &str) -> Option<Module> {
        self.modules.borrow().get(specifier).cloned()
    }
}

fn path_string(path: Option<&Path>) -> Option<String> {
    path.map(|p| p.to_string_lossy().into_owned())
}

impl ModuleLoader for HostModuleLoader {
    async fn load_imported_module(
        self: Rc<Self>,
        referrer: Referrer,
        request: ModuleRequest,
        _context: &RefCell<&mut Context>,
    ) -> JsResult<Module> {
        let specifier = request.specifier().to_std_string_escaped();
        let referrer_url = path_string(referrer.path());
        let dynamic = boa_engine::module::in_dynamic_import();
        let url = match self.resolve(&specifier, referrer_url.as_deref()) {
            Ok(url) => url.to_string(),
            Err(message) => {
                if dynamic {
                    boa_engine::module::record_dynamic_import_failure(&specifier, "", &message);
                }
                return Err(JsNativeError::typ().with_message(message).into());
            }
        };
        // Already parsed: the same module, every time.
        let known = self.modules.borrow().get(&url).cloned();
        if let Some(module) = known {
            return Ok(module);
        }
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.waiting.borrow_mut().push(Waiter {
            url: url.clone(),
            specifier: specifier.clone(),
            dynamic,
            sender: tx,
        });
        if self.asked.borrow_mut().insert(url.clone()) {
            self.requested.borrow_mut().push(RequestedModule {
                url: url.clone(),
                dynamic,
            });
            // The fetch starts here if the call-time hook has not already
            // started it. `start` dedups, so a second notice is a no-op.
            note_module_url(&url, !dynamic, referrer_url.as_deref().unwrap_or(""));
        }
        match rx.await {
            Ok(res) => res,
            Err(_) => {
                if dynamic {
                    boa_engine::module::record_dynamic_import_failure(
                        &specifier,
                        &url,
                        "Module fetch cancelled",
                    );
                }
                Err(JsNativeError::typ()
                    .with_message("Module fetch cancelled")
                    .into())
            }
        }
    }

    fn init_import_meta(
        self: Rc<Self>,
        import_meta: &JsObject,
        module: &Module,
        context: &mut Context,
    ) {
        if let Some(url) = path_string(module.path()) {
            let _ = import_meta.set(js_string!("url"), js_string!(url.as_str()), false, context);
        }
    }
}

/// A dynamic `import()` that rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicImportFailure {
    pub specifier: String,
    /// Resolved URL. Empty when the specifier itself could not be resolved.
    pub url: String,
    pub reason: String,
}

/// A module the embedder started (`JsRuntime::begin_module`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModuleHandle(String);

/// Where a started module is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleState {
    /// Waiting for imports the host has not supplied yet.
    Loading,
    /// Loaded and linked, evaluation not finished (a top-level `await`).
    Evaluating,
    /// Evaluated.
    Done,
    /// The graph could not be loaded or linked: a missing or malformed
    /// dependency, a bare specifier, a link error. No module code ran.
    Failed(String),
    /// The graph loaded and ran, and evaluation threw.
    Threw(String),
}

/// A module import the host has been asked for and has not supplied yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingImport {
    pub url: String,
    pub specifier: String,
    /// `import()`, not a static dependency of the module graph.
    pub dynamic: bool,
}

/// A module's source as the host fetched it.
#[derive(Debug, Clone)]
pub struct FetchedModule {
    /// Where it was finally served from, after redirects: its identity.
    pub final_url: String,
    pub source: String,
}

pub(crate) struct ModuleEntry {
    module: Module,
    load: JsPromise,
    evaluation: Option<JsPromise>,
    failed: Option<String>,
    /// A poll answered `Evaluating` (a top-level `await`) and the outcome
    /// has not been taken since (see [`JsRuntime::take_error_reports`]).
    awaiting: bool,
    /// Its evaluation's outcome has been reported.
    settled: bool,
}

#[derive(Default)]
pub(crate) struct ModuleHost {
    pub(crate) loader: Rc<HostModuleLoader>,
    entries: HashMap<String, ModuleEntry>,
}

impl ModuleHost {
    pub(crate) fn loader(&self) -> Rc<HostModuleLoader> {
        self.loader.clone()
    }
}

fn describe(error: &boa_engine::JsError, context: &mut Context) -> String {
    match error.try_native(context) {
        Ok(native) => native.to_string(),
        Err(_) => crate::describe::error_text(error, context),
    }
}

fn describe_value(value: &JsValue, context: &mut Context) -> String {
    let error = boa_engine::JsError::from_opaque(value.clone());
    describe(&error, context)
}

impl JsRuntime {
    /// The document's URL: the base for resolving a root module's imports.
    pub fn set_module_base(&mut self, url: &str) {
        *self.modules.loader.base.borrow_mut() = Url::parse(url).ok();
    }

    /// Register an import map (`<script type=importmap>`'s text). Returns the
    /// per-entry warnings; an unusable document is an error and changes
    /// nothing. Must come before the modules that need it are started: what is
    /// already resolved stays resolved.
    pub fn add_import_map(&mut self, text: &str) -> Result<Vec<String>, String> {
        let base = self
            .modules
            .loader
            .base
            .borrow()
            .clone()
            .ok_or_else(|| "no document URL to resolve the import map against".to_string())?;
        let (map, warnings) = crate::import_map::ImportMap::parse(text, &base)?;
        self.modules.loader.import_map.borrow_mut().merge(map);
        Ok(warnings)
    }

    /// Parse `source` as the module served from `url` and start loading its
    /// graph. A syntax error is returned here, before any import is asked
    /// for. Starting a URL that is already started returns its handle: the
    /// module map has one entry per URL, so a module is never evaluated twice.
    pub fn begin_module(&mut self, url: &str, source: &str) -> Result<ModuleHandle, String> {
        if self.modules.entries.contains_key(url) {
            return Ok(ModuleHandle(url.to_string()));
        }
        // Static imports (and string-literal `import()`) are known before
        // the parse. The fetch starts now, so it runs while `Module::parse`
        // is still walking this source.
        self.prefetch_source_imports(url, source);
        let existing = self.modules.loader.modules.borrow().get(url).cloned();
        let module = match existing {
            Some(module) => module,
            None => {
                self.diag.know_path(url);
                self.sources.remember(url, source);
                let parsed = parse_module_source(url, source, &mut self.context);
                match parsed {
                    Ok(module) => module,
                    Err(e) => {
                        self.report_error(&e);
                        return Err(describe(&e, &mut self.context));
                    }
                }
            }
        };
        self.modules
            .loader
            .modules
            .borrow_mut()
            .insert(url.to_string(), module.clone());
        let load = module.load(&mut self.context);
        // The engine's promise, not the page's: a failure is the module's
        // error, never an unhandled rejection.
        self.rejections.forget(&JsObject::from(load.clone()));
        let jobs_started = std::time::Instant::now();
        let _ = self.context.run_jobs();
        note_module_phase(url, "jobs", phase_ms(jobs_started));
        self.consume_interrupt();
        self.modules.entries.insert(
            url.to_string(),
            ModuleEntry {
                module,
                load,
                evaluation: None,
                failed: None,
                awaiting: false,
                settled: false,
            },
        );
        Ok(ModuleHandle(url.to_string()))
    }

    /// Dynamic `import()` calls that rejected since the last call: the
    /// specifier, the resolved URL (empty when it did not resolve), and why.
    pub fn take_dynamic_import_failures(&mut self) -> Vec<DynamicImportFailure> {
        boa_engine::module::take_dynamic_import_failures()
            .into_iter()
            .map(|failure| DynamicImportFailure {
                specifier: failure.specifier,
                url: failure.url,
                reason: failure.reason,
            })
            .collect()
    }

    /// Imports whose source has not been supplied. A static one keeps the
    /// module's load promise pending. A dynamic one does not.
    pub fn pending_module_imports(&self) -> Vec<PendingImport> {
        self.modules
            .loader
            .waiting
            .borrow()
            .iter()
            .map(|waiter| PendingImport {
                url: waiter.url.clone(),
                specifier: waiter.specifier.clone(),
                dynamic: waiter.dynamic,
            })
            .collect()
    }

    /// The URLs the graph has asked for since the last call, each at most
    /// once, in the order Boa asked. Includes dynamic `import()`.
    pub fn take_module_requests(&mut self) -> Vec<String> {
        self.take_module_requests_filtered(true)
    }

    /// Static imports only. Dynamic `import()` stays queued for the pump
    /// that runs after `DOMContentLoaded`.
    pub fn take_static_module_requests(&mut self) -> Vec<String> {
        self.take_module_requests_filtered(false)
    }

    fn take_module_requests_filtered(&mut self, include_dynamic: bool) -> Vec<String> {
        let mut requested = self.modules.loader.requested.borrow_mut();
        let mut urls = Vec::new();
        requested.retain(|item| {
            if item.dynamic && !include_dynamic {
                return true;
            }
            urls.push(item.url.clone());
            false
        });
        urls
    }

    /// Give the host's answer for `requested`: its source, or why there is
    /// none. Every import waiting for that URL continues (or fails).
    pub fn supply_module(&mut self, requested: &str, outcome: Result<FetchedModule, String>) {
        let loader = self.modules.loader.clone();
        let module: Result<Module, String> = match outcome {
            Err(message) => Err(message),
            Ok(fetched) => {
                let known = loader.modules.borrow().get(&fetched.final_url).cloned();
                match known {
                    Some(module) => Ok(module),
                    None => {
                        self.diag.know_path(&fetched.final_url);
                        self.sources.remember(&fetched.final_url, &fetched.source);
                        if fetched.final_url != requested {
                            self.sources.remember(requested, &fetched.source);
                        }
                        self.prefetch_source_imports(&fetched.final_url, &fetched.source);
                        match parse_module_source(
                            fetched.final_url.as_str(),
                            fetched.source.as_str(),
                            &mut self.context,
                        ) {
                            Ok(module) => {
                                loader
                                    .modules
                                    .borrow_mut()
                                    .insert(fetched.final_url.clone(), module.clone());
                                Ok(module)
                            }
                            Err(e) => Err(describe(&e, &mut self.context)),
                        }
                    }
                }
            }
        };
        if let Ok(module) = &module {
            // Both the asked-for URL and (above) the final one name it.
            loader
                .modules
                .borrow_mut()
                .insert(requested.to_string(), module.clone());
        }
        // Wake everything waiting for it, in the order they asked. A module
        // reached by a redirect also answers imports of its final URL.
        let waiters: Vec<Waiter> = {
            let mut waiting = loader.waiting.borrow_mut();
            let (ready, rest): (Vec<Waiter>, Vec<Waiter>) =
                std::mem::take(&mut *waiting).into_iter().partition(|w| {
                    w.url == requested
                        || module.as_ref().is_ok_and(|m| {
                            loader.modules.borrow().get(&w.url).is_some_and(|k| k == m)
                        })
                });
            *waiting = rest;
            ready
        };
        for waiter in waiters {
            match &module {
                Ok(module) => {
                    let _ = waiter.sender.send(Ok(module.clone()));
                }
                Err(message) => {
                    if waiter.dynamic {
                        boa_engine::module::record_dynamic_import_failure(
                            &waiter.specifier,
                            &waiter.url,
                            message,
                        );
                    }
                    let _ = waiter.sender.send(Err(JsNativeError::typ()
                        .with_message(format!(
                            "Failed to fetch dynamically imported module: {message}"
                        ))
                        .into()));
                }
            }
        }
        // Page JS resumed by this body (a graph whose load or evaluation
        // was waiting) runs here, before the loader returns. `parsed` on
        // the trace is that return, so this duration is the silent gap
        // between the body and the next module.
        let jobs_started = std::time::Instant::now();
        let _ = self.context.run_jobs();
        note_module_phase(requested, "jobs", phase_ms(jobs_started));
    }

    /// Advance a started module as far as it can go and say where it is:
    /// once its graph is loaded it is linked and evaluated.
    pub fn poll_module(&mut self, handle: &ModuleHandle) -> ModuleState {
        let _loader = push_active_loader(self.modules.loader.clone());
        let jobs_started = std::time::Instant::now();
        let _ = self.context.run_jobs();
        note_module_phase(&handle.0, "jobs", phase_ms(jobs_started));
        self.consume_interrupt();
        let Some(entry) = self.modules.entries.get_mut(&handle.0) else {
            return ModuleState::Failed("unknown module".into());
        };
        if let Some(message) = &entry.failed {
            return ModuleState::Failed(message.clone());
        }
        if entry.evaluation.is_none() {
            match entry.load.state() {
                PromiseState::Pending => return ModuleState::Loading,
                PromiseState::Rejected(reason) => {
                    self.report_value(&reason);
                    let message = describe_value(&reason, &mut self.context);
                    if let Some(entry) = self.modules.entries.get_mut(&handle.0) {
                        entry.failed = Some(message.clone());
                    }
                    return ModuleState::Failed(message);
                }
                PromiseState::Fulfilled(_) => {}
            }
            let module = entry.module.clone();
            let link_started = std::time::Instant::now();
            let linked = module.link(&mut self.context);
            note_module_phase(&handle.0, "link", phase_ms(link_started));
            if let Err(e) = linked {
                self.report_error(&e);
                let message = describe(&e, &mut self.context);
                if let Some(entry) = self.modules.entries.get_mut(&handle.0) {
                    entry.failed = Some(message.clone());
                }
                return ModuleState::Failed(message);
            }
            self.context.reset_recursion_max();
            let eval_started = std::time::Instant::now();
            let evaluated = module.evaluate(&mut self.context);
            note_module_phase(&handle.0, "eval", phase_ms(eval_started));
            let evaluation = match evaluated {
                Ok(p) => {
                    self.rejections.forget(&JsObject::from(p.clone()));
                    p
                }
                Err(e) => {
                    self.report_error(&e);
                    let message = describe(&e, &mut self.context);
                    if let Some(entry) = self.modules.entries.get_mut(&handle.0) {
                        entry.failed = Some(message.clone());
                    }
                    return ModuleState::Failed(message);
                }
            };
            let jobs_started = std::time::Instant::now();
            let _ = self.context.run_jobs();
            note_module_phase(&handle.0, "jobs", phase_ms(jobs_started));
            self.consume_interrupt();
            if let Some(entry) = self.modules.entries.get_mut(&handle.0) {
                entry.evaluation = Some(evaluation);
            }
        }
        let evaluation = match self
            .modules
            .entries
            .get(&handle.0)
            .and_then(|e| e.evaluation.clone())
        {
            Some(p) => p,
            None => return ModuleState::Loading,
        };
        let state = evaluation.state();
        // The outcome is reported once, here or as a late module error.
        let pending = matches!(state, PromiseState::Pending);
        let mut first_outcome = false;
        if let Some(entry) = self.modules.entries.get_mut(&handle.0) {
            entry.awaiting = pending;
            first_outcome = !pending && !entry.settled;
            entry.settled |= !pending;
        }
        match state {
            PromiseState::Pending => ModuleState::Evaluating,
            PromiseState::Fulfilled(_) => ModuleState::Done,
            PromiseState::Rejected(reason) => {
                if first_outcome {
                    self.report_value(&reason);
                }
                ModuleState::Threw(self.thrown_text(&reason))
            }
        }
    }

    /// A module's error, reported once (with reports on).
    fn report_error(&mut self, error: &boa_engine::JsError) {
        if self.diag.on.get() {
            let report = self.diag.describe_error(error, &mut self.context);
            self.diag.push(report);
        }
    }

    fn report_value(&mut self, value: &JsValue) {
        if self.diag.on.get() {
            let report = self.diag.describe(value, None, false, &mut self.context);
            self.diag.push(report);
        }
    }

    /// A module evaluation's rejection as the module's error text, the
    /// engine's own record of the stack after the message, as Boa's text
    /// for a classic script's error has it (read through the realm's
    /// getter: no page code runs).
    fn thrown_text(&mut self, reason: &JsValue) -> String {
        let mut message = describe_value(reason, &mut self.context);
        let getter = self.diag.stack_getter.borrow().clone();
        if let (_, Some(stack)) =
            crate::describe::error_report(reason, getter.as_ref(), &mut self.context)
        {
            message.push('\n');
            message.push_str(stack.trim_end());
        }
        message
    }

    /// The modules a poll left `Evaluating` (a top-level `await` still
    /// waiting) whose evaluation has since thrown: `(module URL, reason)`,
    /// each once, by URL. A browser reports such an error as the module's
    /// uncaught error. Modules that finished cleanly stop being watched.
    pub(crate) fn take_late_module_rejections(&mut self) -> Vec<(String, JsValue)> {
        let mut settled = Vec::new();
        for (url, entry) in self.modules.entries.iter_mut() {
            if !entry.awaiting {
                continue;
            }
            match entry.evaluation.as_ref().map(JsPromise::state) {
                Some(PromiseState::Pending) => {}
                Some(PromiseState::Rejected(reason)) => {
                    entry.awaiting = false;
                    entry.settled = true;
                    settled.push((url.clone(), reason));
                }
                _ => {
                    entry.awaiting = false;
                    entry.settled = true;
                }
            }
        }
        settled.sort_by(|a, b| a.0.cmp(&b.0));
        settled
    }

    /// Tell the embedder about every static import, and every
    /// string-literal `import()`, before this source is parsed.
    fn prefetch_source_imports(&self, module_url: &str, source: &str) {
        let _guard = push_active_loader(self.modules.loader.clone());
        scan_module_imports(module_url, source);
    }
}

/// One URL Boa has asked the host to fetch.
struct RequestedModule {
    url: String,
    dynamic: bool,
}

thread_local! {
    static SCANNING_URL: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// The module whose source is currently being scanned, if any.
pub fn scanning_module_url() -> Option<String> {
    SCANNING_URL.with(|slot| slot.borrow().clone())
}

/// Scan `source` for static imports and string-literal `import()`, and start
/// those fetches. The active loader must already be installed (a module
/// parse, or a nested observe during that scan).
pub fn scan_module_imports(module_url: &str, source: &str) {
    let loader = ACTIVE_LOADER.with(|slot| slot.borrow().clone());
    let Some(loader) = loader else {
        return;
    };
    let _guard = push_active_loader(loader);
    let previous = SCANNING_URL.with(|slot| slot.borrow_mut().replace(module_url.to_string()));
    struct ScanUrlGuard(Option<String>);
    impl Drop for ScanUrlGuard {
        fn drop(&mut self) {
            let previous = self.0.take();
            SCANNING_URL.with(|slot| *slot.borrow_mut() = previous);
        }
    }
    let _url = ScanUrlGuard(previous);
    // Hold the scan open until every specifier in this source is queued,
    // so a dynamic `import()` cannot take a connection ahead of a static
    // import written later in the same file.
    note_module_phase(module_url, "scan-start", 0);
    let scan_started = std::time::Instant::now();
    note_module_scan(true);
    let _scan = ModuleScanGuard;
    for (specifier, dynamic) in import_specifiers(source) {
        prefetch_specifier_kind(module_url, &specifier, !dynamic);
    }
    note_module_phase(module_url, "scan", phase_ms(scan_started));
}

struct ModuleScanGuard;

impl Drop for ModuleScanGuard {
    fn drop(&mut self) {
        note_module_scan(false);
    }
}

thread_local! {
    static ACTIVE_LOADER: RefCell<Option<Rc<HostModuleLoader>>> = const { RefCell::new(None) };
}

/// Process-wide. A thread-local hook is invisible to every test thread but
/// the one that installed it, so the fetch would start only when the import
/// job ran.
static URL_HOOK: std::sync::atomic::AtomicPtr<()> =
    std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());

/// Restores the loader a nested script was resolving imports against.
pub(crate) struct ActiveLoaderGuard {
    previous: Option<Rc<HostModuleLoader>>,
}

impl Drop for ActiveLoaderGuard {
    fn drop(&mut self) {
        let previous = self.previous.take();
        ACTIVE_LOADER.with(|slot| *slot.borrow_mut() = previous);
    }
}

/// Module specifiers resolved while this guard is alive use `loader`.
pub(crate) fn push_active_loader(loader: Rc<HostModuleLoader>) -> ActiveLoaderGuard {
    let previous = ACTIVE_LOADER.with(|slot| slot.borrow_mut().replace(loader));
    ActiveLoaderGuard { previous }
}

/// Called with a resolved module URL as soon as that URL is known.
/// `(url, critical, referrer)`. `referrer` is empty when the importer has
/// no URL. `critical` is false for `import()`.
pub fn set_module_url_hook(hook: Option<fn(&str, bool, &str)>) {
    let ptr = match hook {
        Some(hook) => hook as *mut (),
        None => std::ptr::null_mut(),
    };
    URL_HOOK.store(ptr, std::sync::atomic::Ordering::Relaxed);
}

/// Called while a source is being scanned, so the host can observe module
/// bodies that arrived during the scan instead of after the parse.
pub fn set_module_yield_hook(hook: Option<fn()>) {
    let ptr = match hook {
        Some(hook) => hook as *mut (),
        None => std::ptr::null_mut(),
    };
    YIELD_HOOK.store(ptr, std::sync::atomic::Ordering::Relaxed);
}

static YIELD_HOOK: std::sync::atomic::AtomicPtr<()> =
    std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());

fn note_scan_yield() {
    let ptr = YIELD_HOOK.load(std::sync::atomic::Ordering::Relaxed);
    if ptr.is_null() {
        return;
    }
    // SAFETY: `ptr` was stored from `fn()`, or it is null and we returned.
    let hook: fn() = unsafe { std::mem::transmute(ptr) };
    hook();
}

/// `begin` is true at the start of a source scan and false when it ends.
pub fn set_module_scan_hook(hook: Option<fn(bool)>) {
    let ptr = match hook {
        Some(hook) => hook as *mut (),
        None => std::ptr::null_mut(),
    };
    SCAN_HOOK.store(ptr, std::sync::atomic::Ordering::Relaxed);
}

static SCAN_HOOK: std::sync::atomic::AtomicPtr<()> =
    std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());

fn note_module_scan(begin: bool) {
    let ptr = SCAN_HOOK.load(std::sync::atomic::Ordering::Relaxed);
    if ptr.is_null() {
        return;
    }
    // SAFETY: `ptr` was stored from `fn(bool)`, or it is null and we returned.
    let hook: fn(bool) = unsafe { std::mem::transmute(ptr) };
    hook(begin);
}

/// `(url, phase, ms)`. `phase` is `scan-start`, `scan`, `parse-start`,
/// `parse`, `jobs`, `link`, or `eval`. `ms` is the duration; start phases
/// pass 0.
pub fn set_module_phase_hook(hook: Option<fn(&str, &'static str, u64)>) {
    let ptr = match hook {
        Some(hook) => hook as *mut (),
        None => std::ptr::null_mut(),
    };
    PHASE_HOOK.store(ptr, std::sync::atomic::Ordering::Relaxed);
}

static PHASE_HOOK: std::sync::atomic::AtomicPtr<()> =
    std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());

fn note_module_phase(url: &str, phase: &'static str, ms: u64) {
    let ptr = PHASE_HOOK.load(std::sync::atomic::Ordering::Relaxed);
    if ptr.is_null() || url.is_empty() {
        return;
    }
    // SAFETY: `ptr` was stored from `fn(&str, &'static str, u64)`, or it is null and we returned.
    let hook: fn(&str, &'static str, u64) = unsafe { std::mem::transmute(ptr) };
    hook(url, phase, ms);
}

fn phase_ms(started: std::time::Instant) -> u64 {
    started.elapsed().as_millis() as u64
}

fn note_module_url(url: &str, critical: bool, referrer: &str) {
    let ptr = URL_HOOK.load(std::sync::atomic::Ordering::Relaxed);
    if ptr.is_null() {
        return;
    }
    // SAFETY: `ptr` was stored from `fn(&str, bool, &str)`, or it is null and we returned.
    let hook: fn(&str, bool, &str) = unsafe { std::mem::transmute(ptr) };
    hook(url, critical, referrer);
}

/// Resolve `specifier` and, when it is an http(s) or data URL, start its fetch.
/// Static imports use this. A dynamic `import()` does not.
#[allow(dead_code)]
pub fn prefetch_specifier(referrer: &str, specifier: &str) {
    prefetch_specifier_kind(referrer, specifier, true);
}

fn prefetch_specifier_kind(referrer: &str, specifier: &str, critical: bool) {
    let loader = ACTIVE_LOADER.with(|slot| slot.borrow().clone());
    let Some(loader) = loader else {
        return;
    };
    let referrer = if referrer.is_empty() {
        None
    } else {
        Some(referrer)
    };
    let Ok(url) = loader.resolve(specifier, referrer) else {
        return;
    };
    if matches!(url.scheme(), "http" | "https" | "data") {
        note_module_url(url.as_str(), critical, referrer.unwrap_or(""));
    }
}

/// Install Boa's dynamic-import hook so `import()` starts a fetch when the
/// call is evaluated, before the import job runs.
pub fn install_import_specifier_hook() {
    boa_engine::module::set_import_specifier_hook(Some(|referrer, specifier| {
        prefetch_specifier_kind(referrer, specifier, false);
    }));
}

/// Static `import` / `export from` specifiers, plus string-literal
/// `import("...")`. A computed `import(url)` is not visible here; that
/// fetch starts when the call runs.
#[allow(dead_code)]
pub fn static_import_specifiers(source: &str) -> Vec<String> {
    import_specifiers(source)
        .into_iter()
        .map(|(specifier, _)| specifier)
        .collect()
}

/// How often a scan or a parse looks up module bodies that have already
/// arrived. Boa's parser reads the source as it goes, so the same interval
/// covers `Module::parse`, not only the import scan.
const YIELD_EVERY_BYTES: usize = 64 * 1024;

/// A reader that asks the host to notice ready module bodies every
/// [`YIELD_EVERY_BYTES`]. `Module::parse` otherwise holds the main thread
/// until the whole source is compiled, and a body that arrived during that
/// compile is not scanned until it returns.
struct YieldingSource<'a> {
    data: &'a [u8],
    pos: usize,
    since: usize,
}

impl<'a> YieldingSource<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            since: 0,
        }
    }
}

impl Read for YieldingSource<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.pos >= self.data.len() || buf.is_empty() {
            return Ok(0);
        }
        if self.since >= YIELD_EVERY_BYTES {
            self.since = 0;
            note_scan_yield();
        }
        let n = buf.len().min(self.data.len() - self.pos);
        buf[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
        self.pos += n;
        self.since += n;
        Ok(n)
    }
}

fn parse_module_source(url: &str, source: &str, context: &mut Context) -> JsResult<Module> {
    let path = Path::new(url);
    let reader = YieldingSource::new(source.as_bytes());
    note_module_phase(url, "parse-start", 0);
    let started = std::time::Instant::now();
    let parsed = Module::parse(Source::from_reader(reader, Some(path)), None, context);
    note_module_phase(url, "parse", phase_ms(started));
    parsed
}

/// `(specifier, dynamic)`. `dynamic` is a string-literal `import()`.
fn import_specifiers(source: &str) -> Vec<(String, bool)> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let mut next_yield = YIELD_EVERY_BYTES;
    while i < bytes.len() {
        if i >= next_yield {
            next_yield = i.saturating_add(YIELD_EVERY_BYTES);
            note_scan_yield();
        }
        let b = bytes[i];
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if b == b'/' && bytes.get(i + 1) == Some(&b'/') {
            i += 2;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if b == b'/' && bytes.get(i + 1) == Some(&b'*') {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            continue;
        }
        if matches!(b, b'\'' | b'"' | b'`') {
            i = skip_string(bytes, i);
            continue;
        }
        if is_ident_start(b) {
            let start = i;
            i += 1;
            while i < bytes.len() && is_ident_continue(bytes[i]) {
                i += 1;
            }
            if bytes[start..i] == b"import"[..] {
                let j = skip_ws(bytes, i);
                // `import.meta` is not a module specifier. Treating it as
                // one walked every identifier up to the next `;`.
                if bytes.get(j) == Some(&b'.') {
                    continue;
                }
                if bytes.get(j) == Some(&b'(') {
                    let j = skip_ws(bytes, j + 1);
                    if let Some((specifier, next)) = read_string_literal(bytes, j) {
                        out.push((specifier, true));
                        i = next;
                    }
                    continue;
                }
                if let Some((specifier, next)) = read_string_literal(bytes, j) {
                    out.push((specifier, false));
                    i = next;
                    continue;
                }
                if let Some(specifier) = from_specifier(bytes, i) {
                    out.push((specifier, false));
                }
                continue;
            }
            if bytes[start..i] == b"export"[..] {
                if let Some(specifier) = from_specifier(bytes, i) {
                    out.push((specifier, false));
                }
            }
            continue;
        }
        if b >= 0x80 {
            i = skip_utf8(bytes, i);
            continue;
        }
        i += 1;
    }
    out
}

fn is_ident_start(b: u8) -> bool {
    b == b'_' || b == b'$' || b.is_ascii_alphabetic()
}

fn is_ident_continue(b: u8) -> bool {
    is_ident_start(b) || b.is_ascii_digit()
}

fn skip_ws(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

fn skip_utf8(bytes: &[u8], i: usize) -> usize {
    let b = bytes[i];
    let width = if b & 0xE0 == 0xC0 {
        2
    } else if b & 0xF0 == 0xE0 {
        3
    } else if b & 0xF8 == 0xF0 {
        4
    } else {
        1
    };
    (i + width).min(bytes.len())
}

fn skip_string(bytes: &[u8], start: usize) -> usize {
    let quote = bytes[start];
    let mut i = start + 1;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == quote {
            return i + 1;
        }
        if quote == b'`' && bytes[i] == b'$' && bytes.get(i + 1) == Some(&b'{') {
            i += 2;
            let mut depth = 1i32;
            while i < bytes.len() && depth > 0 {
                if bytes[i] == b'{' {
                    depth += 1;
                    i += 1;
                    continue;
                }
                if bytes[i] == b'}' {
                    depth -= 1;
                    i += 1;
                    continue;
                }
                if matches!(bytes[i], b'\'' | b'"' | b'`') {
                    i = skip_string(bytes, i);
                    continue;
                }
                if bytes[i] == b'\\' {
                    i += 2;
                    continue;
                }
                i += 1;
            }
            continue;
        }
        i += 1;
    }
    bytes.len()
}

fn read_string_literal(bytes: &[u8], start: usize) -> Option<(String, usize)> {
    let quote = *bytes.get(start)?;
    if quote != b'\'' && quote != b'"' {
        return None;
    }
    let mut raw = Vec::new();
    let mut i = start + 1;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            if let Some(next) = bytes.get(i + 1).copied() {
                raw.push(next);
                i += 2;
                continue;
            }
        }
        if bytes[i] == quote {
            return Some((String::from_utf8_lossy(&raw).into_owned(), i + 1));
        }
        if bytes[i] == b'\n' {
            return None;
        }
        raw.push(bytes[i]);
        i += 1;
    }
    None
}

/// The string after the first top-level `from` before the next `;`.
fn from_specifier(bytes: &[u8], mut i: usize) -> Option<String> {
    let mut depth = 0i32;
    while i < bytes.len() {
        if bytes[i] == b';' && depth == 0 {
            return None;
        }
        if bytes[i] == b'{' {
            depth += 1;
            i += 1;
            continue;
        }
        if bytes[i] == b'}' {
            depth -= 1;
            i += 1;
            continue;
        }
        if matches!(bytes[i], b'\'' | b'"' | b'`') {
            i = skip_string(bytes, i);
            continue;
        }
        if depth == 0 && is_ident_start(bytes[i]) {
            let start = i;
            i += 1;
            while i < bytes.len() && is_ident_continue(bytes[i]) {
                i += 1;
            }
            if bytes[start..i] == b"from"[..] {
                let j = skip_ws(bytes, i);
                return read_string_literal(bytes, j).map(|(specifier, _)| specifier);
            }
            continue;
        }
        if bytes[i] >= 0x80 {
            i = skip_utf8(bytes, i);
            continue;
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod specifier_tests {
    use super::static_import_specifiers;

    #[test]
    fn static_and_literal_dynamic_imports_are_visible_before_parse() {
        let source = r#"
            import './a.js';
            import value from "./b.js";
            import { c } from './c.js';
            export { d } from "./d.js";
            import("./e.js");
            import(built);
            const text = "import './nope.js'";
        "#;
        assert_eq!(
            static_import_specifiers(source),
            vec![
                "./a.js".to_string(),
                "./b.js".to_string(),
                "./c.js".to_string(),
                "./d.js".to_string(),
                "./e.js".to_string(),
            ]
        );
    }

    #[test]
    fn import_meta_is_not_a_module_specifier() {
        let source = r#"
            SML.di(`./chunk.js`, import.meta.url, `chunk`);
            import "./real.js";
            const note = "import.meta.url";
        "#;
        assert_eq!(
            static_import_specifiers(source),
            vec!["./real.js".to_string()]
        );
    }
}

#[cfg(test)]
mod host_tests {
    use super::ModuleState;
    use crate::JsRuntime;

    #[test]
    fn a_dynamic_import_is_not_a_load_dependency() {
        let mut rt = JsRuntime::new().expect("runtime");
        rt.set_module_base("https://example.test/");
        let handle = rt
            .begin_module(
                "https://example.test/app.js",
                "import './a.js'; import('./b.js'); export const ready = true;",
            )
            .expect("parse");
        let pending = rt.pending_module_imports();
        assert!(
            pending
                .iter()
                .any(|item| item.url.ends_with("/a.js") && !item.dynamic),
            "{pending:?}"
        );
        assert!(
            pending.iter().all(|item| !item.url.ends_with("/b.js")),
            "dynamic import() held the load promise: {pending:?}"
        );
        assert_eq!(rt.poll_module(&handle), ModuleState::Loading);
    }

    /// Release-mode cost of parsing a bundle the size of reddit's `Bv…` chunk.
    /// Debug builds skip the timing assert; `PARSE_358KB_MS` is the measurement.
    #[test]
    fn parse_time_of_a_358kb_module() {
        let mut body = String::from("export default function bundle(){");
        while body.len() < 358 * 1024 {
            body.push_str("var a=1;a=a+1;");
        }
        body.push_str("return a;}");
        let mut rt = JsRuntime::new().expect("runtime");
        rt.set_module_base("https://example.test/");
        let started = std::time::Instant::now();
        rt.begin_module("https://example.test/bundle.js", &body)
            .expect("parse");
        let ms = started.elapsed().as_millis();
        eprintln!("PARSE_358KB_MS {ms} bytes {}", body.len());
        if !cfg!(debug_assertions) {
            assert!(ms < 8_000, "release parse of 358KB took {ms} ms");
        }
    }

    #[test]
    fn a_large_module_parse_yields_while_it_reads() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static HITS: AtomicUsize = AtomicUsize::new(0);
        struct Clear;
        impl Drop for Clear {
            fn drop(&mut self) {
                super::set_module_yield_hook(None);
            }
        }
        let _clear = Clear;
        HITS.store(0, Ordering::Relaxed);
        super::set_module_yield_hook(Some(|| {
            HITS.fetch_add(1, Ordering::Relaxed);
        }));
        let mut body = String::from("export const value = 1;");
        while body.len() < 200 * 1024 {
            body.push_str("void 0;");
        }
        let mut rt = JsRuntime::new().expect("runtime");
        rt.set_module_base("https://example.test/");
        rt.begin_module("https://example.test/big.js", &body)
            .expect("parse");
        let hits = HITS.load(Ordering::Relaxed);
        assert!(
            hits >= 2,
            "parse of {} bytes yielded {hits} times",
            body.len()
        );
    }

    /// The public Bv concat, fetched as its three chunk files. Debug times
    /// are slower than a Mac release build; the split is what matters.
    #[test]
    fn reddit_bv_body_splits_scan_from_parse_and_jobs() {
        let dir = std::path::Path::new("/tmp/reddit-chunks");
        let ids = ["BvOWmaNPEJ.js", "DZDBx0nCO6.js", "Dyg3q1gB6W.js"];
        if ids.iter().any(|name| !dir.join(name).is_file()) {
            eprintln!("BV_BENCH skipped (no /tmp/reddit-chunks)");
            return;
        }
        let mut body = String::new();
        for name in ids {
            if !body.is_empty() {
                body.push_str("\n/* @sml-chunk */\n");
            }
            body.push_str(&std::fs::read_to_string(dir.join(name)).unwrap());
        }
        let started = std::time::Instant::now();
        let specs = super::import_specifiers(&body);
        let scan_ms = started.elapsed().as_millis();
        let url = "https://www.redditstatic.com/js/concat?prefix=en-US/&chunks=BvOWmaNPEJ,DZDBx0nCO6,Dyg3q1gB6W";
        let parent = "https://www.redditstatic.com/parent.js";
        let mut rt = JsRuntime::new().expect("runtime");
        rt.set_module_base("https://www.redditstatic.com/");
        let started = std::time::Instant::now();
        let handle = rt
            .begin_module(parent, &format!("import '{url}';"))
            .expect("parent");
        let parent_ms = started.elapsed().as_millis();
        assert_eq!(rt.poll_module(&handle), ModuleState::Loading);
        let started = std::time::Instant::now();
        rt.supply_module(
            url,
            Ok(super::FetchedModule {
                final_url: url.to_string(),
                source: body.clone(),
            }),
        );
        let supply_ms = started.elapsed().as_millis();
        let started = std::time::Instant::now();
        let state = rt.poll_module(&handle);
        let poll_ms = started.elapsed().as_millis();
        eprintln!(
            "BV_BENCH bytes={} specs={} scan_ms={scan_ms} parent_ms={parent_ms} supply_ms={supply_ms} poll_ms={poll_ms} state={state:?}",
            body.len(),
            specs.len()
        );
    }
}
