//! WebAssembly for RustKit, on wasmi (WASM W1; see docs/design/WASM-W1.md).
//!
//! Phase 1: `validate`, `Module`, `Instance`, function imports and exports with
//! numeric values, the error classes, the promise forms, and JS <-> wasm
//! re-entry. Memory, Table and Global arrive in later phases.
//!
//! Ownership (the #1440 lesson): each realm's [`WasmRealm`] (its wasmi `Store`,
//! which holds the JS functions given as imports) is owned by the embedder (the
//! `JsRuntime`) and dropped with the realm. The native functions installed in
//! the realm hold only a `Weak` to it, and wasmi host closures capture only
//! plain data (they must be `Send + Sync`). Nothing here roots a realm from
//! inside the garbage-collected graph.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use boa_engine::object::builtins::{JsArray, JsArrayBuffer};
use boa_engine::{
    js_string, Context, JsBigInt, JsError, JsNativeError, JsResult, JsValue, NativeFunction,
};
use wasmi::{TrapCode, ValType};
use wasmi::{AsContext, AsContextMut};

/// The deepest JS <-> wasm nesting before a call throws `RangeError`, as
/// Chrome's stack overflow does (Chrome 148: an unbounded JS <-> wasm
/// recursion is a catchable `RangeError: Maximum call stack size exceeded`).
pub const MAX_NESTING: usize = 256;

/// The most linear memory one realm may allocate (docs/design/WASM-W1.md):
/// past it, `new Memory` is a `RangeError`, as Chrome's is when its own
/// reservation fails.
pub const MAX_REALM_MEMORY: usize = 1 << 30;

/// One page of linear memory.
const PAGE: usize = 65536;

type Caller = wasmi::Caller<'static, HostData>;

thread_local! {
    static NEXT_REALM: Cell<u64> = const { Cell::new(1) };
    /// Per realm (by id): the live `Caller` of each host function running on
    /// this thread, innermost last. A nested export call, or an
    /// instantiation inside a wasm call, goes through the top one.
    static CALLERS: RefCell<HashMap<u64, Vec<*mut Caller>>> = RefCell::new(HashMap::new());
}

/// What a realm's store carries for its host functions.
pub struct HostData {
    realm: u64,
    /// The JS functions given as imports, by the index a host closure holds.
    imports: Vec<boa_engine::JsObject>,
    /// The Boa context of each JS -> wasm call level in progress, innermost
    /// last. Valid exactly while that level's call runs.
    contexts: Vec<*mut Context>,
    /// An exception a JS import threw, carried out through the wasm frames to
    /// the export call that started them, which rethrows it unchanged.
    pending: Option<JsError>,
    /// The store's resource limiter (`Limits`).
    limits: Limits,
}

/// The store's `ResourceLimiter`. W1 memories do not grow
/// (docs/design/WASM-W1.md): a JS-made memory runs over a static buffer of its
/// initial pages, and wasmi 2.0.0 PANICS when `memory.grow` asks a static
/// buffer for more than it holds ("tried to use too small static buffer").
/// Denied here instead, before the buffer is asked, which wasm sees as -1:
/// any growth past the current size is refused; a memory's first allocation
/// (from 0) is allowed within the realm's cap (`MAX_REALM_MEMORY`), which so
/// also bounds the memories wasmi allocates for module-defined memories.
struct Limits {
    allocated: usize,
}

impl wasmi::ResourceLimiter for Limits {
    fn memory_growing(
        &mut self,
        current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> Result<bool, wasmi_core::LimiterError> {
        if current != 0 || desired <= current {
            return Ok(desired <= current);
        }
        if self.allocated.saturating_add(desired) > MAX_REALM_MEMORY {
            return Ok(false);
        }
        self.allocated += desired;
        Ok(true)
    }

    fn table_growing(
        &mut self,
        current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> Result<bool, wasmi_core::LimiterError> {
        // Tables are Vec-backed in wasmi and may grow (within their maximum,
        // which wasmi checks first).
        let _ = (current, desired);
        Ok(true)
    }

    fn instances(&self) -> usize {
        10_000
    }

    fn tables(&self) -> usize {
        10_000
    }

    fn memories(&self) -> usize {
        10_000
    }
}

/// One realm's WebAssembly state.
pub struct WasmRealm {
    id: u64,
    engine: wasmi::Engine,
    store: RefCell<wasmi::Store<HostData>>,
    /// Compiled modules, with each exported function's index in the module's
    /// function space (an exported function's JS `name`, as in Chrome).
    ///
    /// These tables (and `funcs`, and `HostData::imports`) only grow: an entry
    /// per compile, instantiation and import, freed at `drop_realm` (#1453 R1
    /// nit 4). Not a leak across realms (navigation releases them), but a page
    /// instantiating in a loop keeps every entry until then. Phase 2 frees
    /// them with their owning `Module` / `Instance` objects.
    modules: RefCell<Vec<(wasmi::Module, HashMap<String, u32>)>>,
    funcs: RefCell<Vec<wasmi::Func>>,
    /// Memories by id: the wasmi memory and, for one made in JS, the
    /// `ArrayBuffer` whose storage it runs over (`Memory::new_static`). Held
    /// here so the bytes can never be freed while this store, and so any
    /// instance using them, lives; dropped with the realm (Rust ownership,
    /// not a root inside the GC graph). A module-defined memory has no
    /// buffer in W1 (its exported `buffer` is a TypeError, stated).
    memories: RefCell<Vec<(wasmi::Memory, Option<boa_engine::JsObject>)>>,
    /// Bytes of linear memory allocated in this realm (`MAX_REALM_MEMORY`).
    memory_bytes: Cell<usize>,
    /// Tables and globals by id.
    tables: RefCell<Vec<wasmi::Table>>,
    globals: RefCell<Vec<wasmi::Global>>,
    /// A function's id by its handle's identity. wasmi's handles derive only
    /// Debug/Copy/Clone (no equality); their Debug form names the store and
    /// the index, a stable identity, so the same function (an export, or one
    /// read from a table) always answers the same id, and so the same JS
    /// function object (Chrome: `table.get(i) === exports.f`).
    func_ids: RefCell<HashMap<String, usize>>,
    /// The prelude's `convert` (setConvert): turns a native error string
    /// into its WebAssembly error class. Called only on an exported
    /// function's error path; held here, not in the export closures, so no
    /// closure holds a GC root (#1440), and released with the realm.
    convert: RefCell<Option<boa_engine::JsObject>>,
}

impl WasmRealm {
    pub fn new() -> Rc<Self> {
        // Stated W1 gaps (docs/design/WASM-W1.md): no memory64, no SIMD. Both
        // are left out at build time (wasmi without its `memory64` and `simd`
        // features), so a module using either fails validation.
        // Fuel: every call runs in slices of FUEL_SLICE, and between slices
        // the JS context's interrupt word is checked (`call_func`), so the
        // turn's budget stops wasm where it would stop script.
        //
        // Compilation is eager: wasmi's default compiles each function at its
        // first call and charges that compile fuel as an error that can't be
        // resumed, so a big function (or a slice nearly used up) became a bare
        // trap mid-run. Eager compilation does it all in `WebAssembly.Module`,
        // where no slice is running.
        let mut config = wasmi::Config::default();
        config.consume_fuel(true);
        config.compilation_mode(wasmi::CompilationMode::Eager);
        let engine = wasmi::Engine::new(&config);
        let id = NEXT_REALM.with(|n| {
            let id = n.get();
            n.set(id + 1);
            id
        });
        let data = HostData { realm: id, imports: Vec::new(), contexts: Vec::new(), pending: None, limits: Limits { allocated: 0 } };
        let mut store = wasmi::Store::new(&engine, data);
        store.limiter(|data| &mut data.limits);
        Rc::new(Self {
            id,
            engine,
            store: RefCell::new(store),
            modules: RefCell::default(),
            funcs: RefCell::default(),
            memories: RefCell::default(),
            memory_bytes: Cell::new(0),
            tables: RefCell::default(),
            globals: RefCell::default(),
            func_ids: RefCell::default(),
            convert: RefCell::default(),
        })
    }

    /// The live `Caller` of the innermost host function of this realm
    /// running on this thread, if wasm is running below us.
    fn top_caller(&self) -> Option<*mut Caller> {
        CALLERS.with(|c| c.borrow().get(&self.id).and_then(|v| v.last().copied()))
    }

    /// Run `f` with this realm's store: through the live `Caller` when wasm
    /// is running below us (re-entry), else through the store itself.
    fn with_store<R>(&self, f: impl FnOnce(&mut dyn StoreLike) -> R) -> R {
        match self.top_caller() {
            // SAFETY: the pointer is the `Caller` of a host function running
            // below us on this thread; it lives until that host function
            // returns, which is after `f` (the host function called us).
            Some(caller) => f(unsafe { &mut *caller }),
            None => f(&mut *self.store.borrow_mut()),
        }
    }
}

impl Drop for WasmRealm {
    fn drop(&mut self) {
        CALLERS.with(|c| {
            c.borrow_mut().remove(&self.id);
        });
    }
}

/// A store or a `Caller`: what a call or an instantiation goes through.
trait StoreLike {
    fn ctx(&mut self) -> wasmi::StoreContextMut<'_, HostData>;
}

impl StoreLike for wasmi::Store<HostData> {
    fn ctx(&mut self) -> wasmi::StoreContextMut<'_, HostData> {
        self.as_context_mut()
    }
}

impl StoreLike for Caller {
    fn ctx(&mut self) -> wasmi::StoreContextMut<'_, HostData> {
        self.as_context_mut()
    }
}

/// Each exported function's index in the module's function space (its JS
/// `name`, as in Chrome), from the export section (id 7). wasmi 2.0.0 keeps
/// that index private (`ExportType` has only `name` and `ty`), so the section
/// is read here. The bytes were validated by wasmi first; even so every step
/// is bounds-checked, and anything malformed just ends the walk.
fn export_func_indices(bytes: &[u8]) -> HashMap<String, u32> {
    fn leb(b: &[u8], i: &mut usize) -> Option<u32> {
        let (mut r, mut shift) = (0u32, 0u32);
        loop {
            let x = *b.get(*i)?;
            *i += 1;
            if shift >= 32 {
                return None;
            }
            r |= u32::from(x & 0x7f).checked_shl(shift)?;
            if x & 0x80 == 0 {
                return Some(r);
            }
            shift += 7;
        }
    }
    let mut out = HashMap::new();
    let mut i = 8usize;
    while i < bytes.len() {
        let id = bytes[i];
        i += 1;
        let Some(size) = leb(bytes, &mut i) else { break };
        let Some(end) = i.checked_add(size as usize).filter(|&e| e <= bytes.len()) else { break };
        if id == 7 {
            let section = &bytes[i..end];
            let mut j = 0usize;
            let n = leb(section, &mut j).unwrap_or(0);
            for _ in 0..n {
                let Some(len) = leb(section, &mut j) else { break };
                let Some(stop) = j.checked_add(len as usize).filter(|&e| e <= section.len()) else { break };
                let Ok(name) = std::str::from_utf8(&section[j..stop]) else { break };
                j = stop;
                let Some(&kind) = section.get(j) else { break };
                j += 1;
                let Some(index) = leb(section, &mut j) else { break };
                if kind == 0 {
                    out.insert(name.to_string(), index);
                }
            }
            break;
        }
        i = end;
    }
    out
}

/// The id of `func` in `realm.funcs`, added if new (`WasmRealm::func_ids`).
fn func_id(realm: &WasmRealm, func: wasmi::Func) -> usize {
    let key = format!("{func:?}");
    if let Some(id) = realm.func_ids.borrow().get(&key) {
        return *id;
    }
    let mut funcs = realm.funcs.borrow_mut();
    funcs.push(func);
    let id = funcs.len() - 1;
    realm.func_ids.borrow_mut().insert(key, id);
    id
}

/// A funcref for JS: `[funcId, arity]`, or `null`.
fn funcref_to_js(realm: &WasmRealm, r: wasmi::Ref, context: &mut Context) -> JsResult<JsValue> {
    let func = match r {
        wasmi::Ref::Func(f) => Option::<wasmi::Func>::from(f),
        _ => return Err(JsNativeError::typ().with_message("WebAssembly: externref arrives later in W1 phase 3").into()),
    };
    let Some(func) = func else { return Ok(JsValue::null()) };
    let id = func_id(realm, func);
    let arity = realm.with_store(|s| func.ty(s.ctx()).params().len());
    let row: [JsValue; 2] = [JsValue::from(id as u32), JsValue::from(arity as u32)];
    Ok(JsArray::from_iter(row, context).into())
}

/// A funcref from JS: the prelude hands a wasm function over as its id, null as -1.
fn funcref_from_js(realm: &WasmRealm, value: &JsValue) -> JsResult<wasmi::Ref> {
    let id = value.as_number().unwrap_or(-1.0);
    if id < 0.0 {
        return Ok(wasmi::Ref::Func(wasmi::Nullable::Null));
    }
    let func = realm.funcs.borrow().get(id as usize).copied().ok_or_else(JsNativeError::typ)?;
    Ok(wasmi::Ref::Func(wasmi::Nullable::Val(func)))
}

fn val_type(name: &str) -> Option<ValType> {
    Some(match name {
        "i32" => ValType::I32,
        "i64" => ValType::I64,
        "f32" => ValType::F32,
        "f64" => ValType::F64,
        _ => return None,
    })
}

fn realm_of(weak: &Weak<WasmRealm>) -> JsResult<Rc<WasmRealm>> {
    weak.upgrade().ok_or_else(|| JsNativeError::typ().with_message("WebAssembly: the realm has gone").into())
}

/// The bytes of an `ArrayBuffer` (the prelude hands every BufferSource over as one).
fn bytes_of(value: &JsValue) -> JsResult<Vec<u8>> {
    let object = value.as_object().ok_or_else(|| JsNativeError::typ().with_message("not an ArrayBuffer"))?;
    let buffer = JsArrayBuffer::from_object(object)?;
    let data = buffer.data().ok_or_else(|| JsNativeError::typ().with_message("detached ArrayBuffer"))?;
    Ok(data.to_vec())
}

/// A WebAssembly error for the prelude to turn into its class: a string
/// `"\u{1}wasm\u{1}<Class>\u{1}<message>"`, never seen by page script (the
/// prelude's wrappers catch it at the boundary they called).
fn wasm_error(kind: &str, message: impl std::fmt::Display) -> JsError {
    JsError::from_opaque(js_string!(format!("\u{1}wasm\u{1}{kind}\u{1}{message}")).into())
}

/// V8's wording for a trap (Chrome 148: `unreachable` for the instruction).
fn trap_message(code: TrapCode) -> &'static str {
    match code {
        TrapCode::UnreachableCodeReached => "unreachable",
        TrapCode::MemoryOutOfBounds => "memory access out of bounds",
        TrapCode::TableOutOfBounds => "table index is out of bounds",
        // Chrome 148: a null table entry is "null function".
        TrapCode::IndirectCallToNull => "null function",
        TrapCode::BadSignature => "null function or function signature mismatch",
        TrapCode::IntegerDivisionByZero => "divide by zero",
        TrapCode::IntegerOverflow => "divide result unrepresentable",
        TrapCode::BadConversionToInteger => "float unrepresentable in integer range",
        _ => "trap",
    }
}

fn overflow() -> JsError {
    JsNativeError::range().with_message("Maximum call stack size exceeded").into()
}

fn to_wasm(value: &JsValue, ty: ValType, context: &mut Context) -> JsResult<wasmi::Val> {
    Ok(match ty {
        ValType::I32 => wasmi::Val::I32(value.to_i32(context)?),
        ValType::I64 => wasmi::Val::I64(value.to_big_int64(context)?),
        ValType::F32 => wasmi::Val::F32((value.to_number(context)? as f32).into()),
        ValType::F64 => wasmi::Val::F64(value.to_number(context)?.into()),
        _ => {
            return Err(JsNativeError::typ()
                .with_message("WebAssembly: reference values arrive in W1 phase 3")
                .into())
        }
    })
}

fn to_js(value: &wasmi::Val) -> JsResult<JsValue> {
    Ok(match value {
        wasmi::Val::I32(v) => JsValue::from(*v),
        wasmi::Val::I64(v) => JsValue::from(JsBigInt::from(*v)),
        wasmi::Val::F32(v) => JsValue::from(f64::from(f32::from(*v))),
        wasmi::Val::F64(v) => JsValue::from(f64::from(*v)),
        _ => {
            return Err(JsNativeError::typ()
                .with_message("WebAssembly: reference values arrive in W1 phase 3")
                .into())
        }
    })
}

/// Call exported `func` with JS `args`, at the outermost level or re-entered
/// from inside a JS import (through that host function's live `Caller`).
fn call_func(realm: &WasmRealm, func: wasmi::Func, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let ty = realm.with_store(|s| func.ty(s.ctx()));
    let mut params = Vec::with_capacity(ty.params().len());
    for (i, p) in ty.params().iter().enumerate() {
        params.push(to_wasm(args.get(i).unwrap_or(&JsValue::undefined()), *p, context)?);
    }
    let mut results: Vec<wasmi::Val> = ty.results().iter().map(|t| wasmi::Val::default_for_ty(*t)).collect();
    let outcome = realm.with_store(|s| {
        let mut ctx = s.ctx();
        if ctx.data().contexts.len() >= MAX_NESTING {
            return Err((None, Some(overflow())));
        }
        let context_ptr = context as *mut Context;
        ctx.data_mut().contexts.push(context_ptr);
        let r = run_in_slices(&mut ctx, func, &params, &mut results, context_ptr);
        ctx.data_mut().contexts.pop();
        r.map_err(|e| match e {
            Stop::Wasm(e) => (Some(e), ctx.data_mut().pending.take()),
            Stop::Interrupted(stop) => (None, Some(stop)),
        })
    });
    match outcome {
        Ok(()) => match results.len() {
            0 => Ok(JsValue::undefined()),
            1 => to_js(&results[0]),
            _ => {
                let out: Vec<JsValue> = results.iter().map(to_js).collect::<JsResult<_>>()?;
                Ok(JsArray::from_iter(out, context).into())
            }
        },
        // A JS import threw, or the nesting bound was hit: that, unchanged.
        Err((_, Some(pending))) => Err(pending),
        Err((Some(e), None)) => match e.as_trap_code() {
            Some(TrapCode::StackOverflow) => Err(overflow()),
            Some(code) => Err(wasm_error("RuntimeError", trap_message(code))),
            None => Err(wasm_error("RuntimeError", e)),
        },
        Err((None, None)) => Err(wasm_error("RuntimeError", "trap")),
    }
}

/// Fuel per slice: between slices the turn's interrupt word is checked. About
/// a millisecond of wasmi's loop dispatch at a few hundred million
/// instructions a second, so a stop lands within the watchdog's slack.
const FUEL_SLICE: u64 = 1_000_000;

/// Why a sliced call ended early.
enum Stop {
    /// A trap, or a JS import's error (its exception is in `HostData::pending`).
    Wasm(wasmi::Error),
    /// The JS context's interrupt word stopped the turn: Boa's own uncatchable error.
    Interrupted(JsError),
}

/// Call `func` in slices of FUEL_SLICE (call_resumable), checking the JS
/// context's interrupt word whenever a slice runs out. Used at every level,
/// outermost and nested inside a JS import alike: a plain call that ran out
/// of fuel would trap instead of being resumed.
fn run_in_slices(
    ctx: &mut wasmi::StoreContextMut<'_, HostData>,
    func: wasmi::Func,
    params: &[wasmi::Val],
    results: &mut [wasmi::Val],
    context: *mut Context,
) -> Result<(), Stop> {
    ctx.set_fuel(FUEL_SLICE).map_err(Stop::Wasm)?;
    let mut call = func.call_resumable(&mut *ctx, params, results).map_err(Stop::Wasm)?;
    loop {
        match call {
            wasmi::ResumableCall::Finished => return Ok(()),
            wasmi::ResumableCall::HostTrap(trap) => return Err(Stop::Wasm(trap.into_host_error())),
            wasmi::ResumableCall::OutOfFuel(paused) => {
                // SAFETY: `context` is the JS context that made this call; it
                // outlives the call (call_func's caller holds it).
                if let Err(stop) = unsafe { &*context }.check_interrupt_now() {
                    return Err(Stop::Interrupted(stop));
                }
                ctx.set_fuel(FUEL_SLICE).map_err(Stop::Wasm)?;
                call = paused.resume(&mut *ctx, results).map_err(Stop::Wasm)?;
            }
        }
    }
}

/// The host function for import `index` of type `ty`: calls the JS function
/// in the context of the call level that started the wasm below us.
fn host_func(ctx: wasmi::StoreContextMut<'_, HostData>, ty: wasmi::FuncType, index: usize) -> wasmi::Func {
    let result_types: Vec<ValType> = ty.results().to_vec();
    wasmi::Func::new(ctx, ty, move |mut caller, params, results| {
        let context = *caller.data().contexts.last().ok_or_else(|| wasmi::Error::new("WebAssembly: no JS call in progress"))?;
        let function = caller.data().imports[index].clone();
        let realm = caller.data().realm;
        // SAFETY: the context pointer was pushed by the call level running
        // below us on this thread and is popped only after it returns.
        let context = unsafe { &mut *context };
        let args: Vec<JsValue> = match params.iter().map(to_js).collect::<JsResult<_>>() {
            Ok(args) => args,
            Err(e) => {
                caller.data_mut().pending = Some(e);
                return Err(wasmi::Error::new("js"));
            }
        };
        // This host function's `Caller` is the one a nested export call (or
        // an instantiation) of this realm goes through, while the JS runs.
        let raw = (&mut caller as *mut wasmi::Caller<'_, HostData>).cast::<Caller>();
        CALLERS.with(|c| c.borrow_mut().entry(realm).or_default().push(raw));
        let returned = function.call(&JsValue::undefined(), &args, context);
        CALLERS.with(|c| {
            if let Some(v) = c.borrow_mut().get_mut(&realm) {
                v.pop();
            }
        });
        let converted = returned.and_then(|value| match result_types.len() {
            0 => Ok(()),
            1 => {
                results[0] = to_wasm(&value, result_types[0], context)?;
                Ok(())
            }
            n => {
                // Several results: the JS function returns them as an iterable.
                let list = value
                    .as_object()
                    .map(|o| JsArray::from_object(o.clone()))
                    .transpose()?
                    .ok_or_else(|| JsNativeError::typ().with_message("multi-value import must return an array"))?;
                for i in 0..n {
                    results[i] = to_wasm(&list.get(i as u64, context)?, result_types[i], context)?;
                }
                Ok(())
            }
        });
        converted.map_err(|e| {
            caller.data_mut().pending = Some(e);
            wasmi::Error::new("js")
        })
    })
}

/// A table or global a module exports.
enum ExternHandle {
    Table(wasmi::Table),
    Global(wasmi::Global),
}

fn kind_name(ty: &wasmi::ExternType) -> &'static str {
    match ty {
        wasmi::ExternType::Func(_) => "function",
        wasmi::ExternType::Table(_) => "table",
        wasmi::ExternType::Memory(_) => "memory",
        wasmi::ExternType::Global(_) => "global",
    }
}

/// The JS half: a script that evaluates to a function taking the natives
/// object (`natives`) and defining `WebAssembly` on the global object. The
/// embedder evaluates it through its own script path (which sets up Boa's
/// native-stack accounting) and calls the result with the natives.
pub const PRELUDE: &str = include_str!("web_wasm.js");

/// The natives the prelude takes, for `context`'s realm (the current one):
/// `{ validate, compile, reflect, instantiate, call }`.
pub fn natives(context: &mut Context, realm: &Rc<WasmRealm>) -> JsResult<boa_engine::JsObject> {
    let weak = Rc::downgrade(realm);

    // SAFETY (every closure below): they capture a `Weak<WasmRealm>`, plain
    // Rust data the collector need not trace and that roots nothing.
    let w = weak.clone();
    let validate = unsafe {
        NativeFunction::from_closure(move |_, args, _| {
            let realm = realm_of(&w)?;
            let bytes = bytes_of(args.first().unwrap_or(&JsValue::undefined()))?;
            Ok(JsValue::from(wasmi::Module::validate(&realm.engine, &bytes).is_ok()))
        })
    };
    let w = weak.clone();
    let compile = unsafe {
        NativeFunction::from_closure(move |_, args, _| {
            let realm = realm_of(&w)?;
            let bytes = bytes_of(args.first().unwrap_or(&JsValue::undefined()))?;
            let module = wasmi::Module::new(&realm.engine, &bytes).map_err(|e| wasm_error("CompileError", e))?;
            let mut modules = realm.modules.borrow_mut();
            modules.push((module, export_func_indices(&bytes)));
            Ok(JsValue::from((modules.len() - 1) as u32))
        })
    };
    let w = weak.clone();
    // reflect(moduleId, imports?): [[name, kind], ...] or [[module, name, kind], ...]
    let reflect = unsafe {
        NativeFunction::from_closure(move |_, args, context| {
            let realm = realm_of(&w)?;
            let id = args.first().cloned().unwrap_or_default().to_u32(context)? as usize;
            let imports = args.get(1).is_some_and(JsValue::to_boolean);
            let module = realm.modules.borrow().get(id).map(|m| m.0.clone()).ok_or_else(|| JsNativeError::typ().with_message("no such module"))?;
            let rows: Vec<JsValue> = if imports {
                module
                    .imports()
                    .map(|i| {
                        let row: [JsValue; 3] = [
                            js_string!(i.module()).into(),
                            js_string!(i.name()).into(),
                            js_string!(kind_name(i.ty())).into(),
                        ];
                        JsArray::from_iter(row, context).into()
                    })
                    .collect()
            } else {
                module
                    .exports()
                    .map(|e| {
                        let row: [JsValue; 2] = [js_string!(e.name()).into(), js_string!(kind_name(e.ty())).into()];
                        JsArray::from_iter(row, context).into()
                    })
                    .collect()
            };
            Ok(JsArray::from_iter(rows, context).into())
        })
    };
    let w = weak.clone();
    // instantiate(moduleId, [[module, name, value], ...]) -> [[name, funcId, arity, funcIndex], ...]
    let instantiate = unsafe {
        NativeFunction::from_closure(move |_, args, context| {
            let realm = realm_of(&w)?;
            let id = args.first().cloned().unwrap_or_default().to_u32(context)? as usize;
            let (module, indices) = realm.modules.borrow().get(id).cloned().ok_or_else(|| JsNativeError::typ().with_message("no such module"))?;
            let given = args.get(1).and_then(JsValue::as_object).ok_or_else(|| JsNativeError::typ().with_message("imports"))?;
            let given = JsArray::from_object(given)?;
            let mut by_name = HashMap::new();
            for i in 0..given.length(context)? {
                let row = given.get(i, context)?;
                let row = JsArray::from_object(row.as_object().ok_or_else(JsNativeError::typ)?)?;
                let m = row.get(0, context)?.to_string(context)?.to_std_string_escaped();
                let name = row.get(1, context)?.to_string(context)?.to_std_string_escaped();
                by_name.insert((m, name), row.get(2, context)?);
            }
            // A start function may call a JS import: instantiation is a call
            // level of its own, in this native's context.
            let context_ptr = context as *mut Context;
            #[allow(clippy::type_complexity)]
            let linked: JsResult<(
                Vec<(wasmi::Func, String, usize, u32)>,
                Vec<(String, wasmi::Memory)>,
                Vec<(String, &'static str, ExternHandle)>,
            )> = realm.with_store(|s| {
                let mut ctx = s.ctx();
                let mut linker = <wasmi::Linker<HostData>>::new(&realm.engine);
                for import in module.imports() {
                    let key = (import.module().to_string(), import.name().to_string());
                    let value = by_name.get(&key).cloned().unwrap_or_default();
                    match import.ty() {
                        wasmi::ExternType::Func(ty) => {
                            let function = value.as_callable().ok_or_else(|| {
                                wasm_error(
                                    "LinkError",
                                    format!("Import \"{}\" \"{}\": function import requires a callable", key.0, key.1),
                                )
                            })?;
                            let index = ctx.data().imports.len();
                            ctx.data_mut().imports.push(function);
                            let func = host_func(ctx.as_context_mut(), ty.clone(), index);
                            linker.define(&key.0, &key.1, func).map_err(|e| wasm_error("LinkError", e))?;
                        }
                        wasmi::ExternType::Memory(_) => {
                            // The prelude hands a WebAssembly.Memory over as its id, anything else as -1.
                            let id = value.as_number().filter(|n| *n >= 0.0).map(|n| n as usize);
                            let memory = id.and_then(|id| realm.memories.borrow().get(id).map(|m| m.0)).ok_or_else(|| {
                                wasm_error(
                                    "LinkError",
                                    format!("Import \"{}\" \"{}\": memory import must be a WebAssembly.Memory object", key.0, key.1),
                                )
                            })?;
                            linker.define(&key.0, &key.1, memory).map_err(|e| wasm_error("LinkError", e))?;
                        }
                        wasmi::ExternType::Table(_) => {
                            let id = value.as_number().filter(|n| *n >= 0.0).map(|n| n as usize);
                            let table = id.and_then(|id| realm.tables.borrow().get(id).copied()).ok_or_else(|| {
                                wasm_error(
                                    "LinkError",
                                    format!("Import \"{}\" \"{}\": table import requires a WebAssembly.Table", key.0, key.1),
                                )
                            })?;
                            linker.define(&key.0, &key.1, table).map_err(|e| wasm_error("LinkError", e))?;
                        }
                        wasmi::ExternType::Global(ty) => {
                            // The prelude hands ["G", globalId] for a WebAssembly.Global, ["V", value] otherwise.
                            let row = value.as_object().map(JsArray::from_object).transpose()?.ok_or_else(JsNativeError::typ)?;
                            let tag = row.get(0, &mut *unsafe { &mut *context_ptr })?.to_string(unsafe { &mut *context_ptr })?.to_std_string_escaped();
                            let payload = row.get(1, unsafe { &mut *context_ptr })?;
                            let global = if tag == "G" {
                                let id = payload.as_number().unwrap_or(-1.0);
                                realm.globals.borrow().get(id as usize).copied().ok_or_else(JsNativeError::typ)?
                            } else {
                                let link = |why: &str| wasm_error("LinkError", format!("Import \"{}\" \"{}\": {why}", key.0, key.1));
                                if ty.mutability().is_mut() {
                                    return Err(link("imported mutable global must be a WebAssembly.Global object"));
                                }
                                let content = ty.content();
                                let ok = match content {
                                    ValType::I64 => payload.is_bigint(),
                                    ValType::I32 | ValType::F32 | ValType::F64 => payload.is_number(),
                                    _ => false,
                                };
                                if !ok {
                                    return Err(link("global import must be a number, valid Wasm reference, or WebAssembly.Global object"));
                                }
                                let v = to_wasm(&payload, content, unsafe { &mut *context_ptr })?;
                                wasmi::Global::new(ctx.as_context_mut(), v, wasmi::Mutability::Const)
                            };
                            linker.define(&key.0, &key.1, global).map_err(|e| wasm_error("LinkError", e))?;
                        }
                    }
                }
                ctx.data_mut().contexts.push(context_ptr);
                // wasmi runs a start function with a plain call, which can't be
                // resumed after a slice: it gets unlimited fuel, so a start
                // function that never ends is not stopped (stated W1 gap; a
                // wasm call already running around this one gets a fresh slice
                // back below and is stopped at its next one).
                let _ = ctx.set_fuel(u64::MAX);
                let started = linker.instantiate_and_start(&mut ctx, &module);
                let _ = ctx.set_fuel(FUEL_SLICE);
                ctx.data_mut().contexts.pop();
                let instance = started.map_err(|e| match ctx.data_mut().pending.take() {
                    // The start function's JS import threw: that, unchanged.
                    Some(pending) => pending,
                    None => match e.as_trap_code() {
                        Some(code) => wasm_error("RuntimeError", trap_message(code)),
                        None => wasm_error("LinkError", e),
                    },
                })?;
                let mut out = Vec::new();
                let mut memories = Vec::new();
                let mut others = Vec::new();
                for export in module.exports() {
                    if let Some(func) = instance.get_func(ctx.as_context(), export.name()) {
                        let arity = func.ty(ctx.as_context()).params().len();
                        let index = indices.get(export.name()).copied().unwrap_or(0);
                        out.push((func, export.name().to_string(), arity, index));
                    } else if let Some(memory) = instance.get_memory(ctx.as_context(), export.name()) {
                        memories.push((export.name().to_string(), memory));
                    } else if let Some(table) = instance.get_table(ctx.as_context(), export.name()) {
                        others.push((export.name().to_string(), "table", ExternHandle::Table(table)));
                    } else if let Some(global) = instance.get_global(ctx.as_context(), export.name()) {
                        others.push((export.name().to_string(), "global", ExternHandle::Global(global)));
                    }
                }
                Ok((out, memories, others))
            });
            let (linked, exported_memories, others) = linked?;
            let mut rows = Vec::new();
            // Table and global exports: ["name", "table" | "global", id].
            for (name, kind, handle) in others {
                let id = match handle {
                    ExternHandle::Table(t) => {
                        let mut tables = realm.tables.borrow_mut();
                        let key = format!("{t:?}");
                        match tables.iter().position(|x| format!("{x:?}") == key) {
                            Some(i) => i,
                            None => {
                                tables.push(t);
                                tables.len() - 1
                            }
                        }
                    }
                    ExternHandle::Global(g) => {
                        let mut globals = realm.globals.borrow_mut();
                        let key = format!("{g:?}");
                        match globals.iter().position(|x| format!("{x:?}") == key) {
                            Some(i) => i,
                            None => {
                                globals.push(g);
                                globals.len() - 1
                            }
                        }
                    }
                };
                let row: [JsValue; 3] = [js_string!(name).into(), js_string!(kind).into(), JsValue::from(id as u32)];
                rows.push(JsArray::from_iter(row, context).into());
            }
            // A memory export: ["name", "memory", memId]. An imported memory
            // keeps its id (the prelude answers the same Memory object); a
            // module-defined one gets an id with no buffer (W1, stated).
            for (name, memory) in exported_memories {
                // wasmi's handles have no equality: the same memory is the
                // same storage, so compare data pointers.
                let known: Vec<*mut u8> = {
                    let memories = realm.memories.borrow();
                    realm.with_store(|s| memories.iter().map(|m| m.0.data_ptr(s.ctx())).collect())
                };
                let here = realm.with_store(|s| memory.data_ptr(s.ctx()));
                let empty = realm.with_store(|s| memory.size(s.ctx())) == 0;
                let mut memories = realm.memories.borrow_mut();
                // Zero-page memories share a dangling pointer: never matched.
                let id = match known.iter().position(|p| *p == here && !empty) {
                    Some(id) => id,
                    None => {
                        memories.push((memory, None));
                        memories.len() - 1
                    }
                };
                let row: [JsValue; 3] = [js_string!(name).into(), js_string!("memory").into(), JsValue::from(id as u32)];
                rows.push(JsArray::from_iter(row, context).into());
            }
            for (func, name, arity, index) in linked {
                let id = func_id(&realm, func);
                let row: [JsValue; 4] = [
                    js_string!(name).into(),
                    JsValue::from(id as u32),
                    JsValue::from(arity as u32),
                    JsValue::from(index),
                ];
                rows.push(JsArray::from_iter(row, context).into());
            }
            Ok(JsArray::from_iter(rows, context).into())
        })
    };
    let w = weak.clone();
    // setConvert(fn): the prelude's error conversion, for makeExport's functions.
    let set_convert = unsafe {
        NativeFunction::from_closure(move |_, args, _| {
            let realm = realm_of(&w)?;
            *realm.convert.borrow_mut() = args.first().and_then(JsValue::as_object);
            Ok(JsValue::undefined())
        })
    };
    let w = weak.clone();
    // makeExport(funcId, arity, name): an exported function as a native
    // function object (no prototype, not a constructor, as in Chrome). A JS
    // wrapper frame per call cost about as much as the call itself in Boa
    // (JetStream's richards-wasm makes 42.7M of them).
    let make_export = unsafe {
        NativeFunction::from_closure(move |_, args, context| {
            let id = args.first().cloned().unwrap_or_default().to_u32(context)? as usize;
            let arity = args.get(1).cloned().unwrap_or_default().to_u32(context)? as usize;
            let name = args.get(2).cloned().unwrap_or_default().to_string(context)?;
            let weak = w.clone();
            let function = NativeFunction::from_closure(move |_, args, context| {
                let realm = realm_of(&weak)?;
                let func = realm.funcs.borrow().get(id).copied().ok_or_else(|| JsNativeError::typ().with_message("no such function"))?;
                call_func(&realm, func, args, context).or_else(|e| {
                    // Only this crate's error strings (wasm_error) are
                    // converted; a JS import's exception and the turn's
                    // uncatchable stop pass through as they are.
                    let marked = e.as_opaque().filter(|v| v.is_string()).cloned();
                    let convert = realm.convert.borrow().clone();
                    match (marked, convert) {
                        (Some(thrown), Some(convert)) => {
                            Err(JsError::from_opaque(convert.call(&JsValue::undefined(), &[thrown], context)?))
                        }
                        _ => Err(e),
                    }
                })
            });
            let realm = context.realm().clone();
            Ok(boa_engine::object::FunctionObjectBuilder::new(&realm, function)
                .name(name)
                .length(arity)
                .constructor(false)
                .build()
                .into())
        })
    };
    let w = weak.clone();
    // call(funcId, argsArray)
    let call = unsafe {
        NativeFunction::from_closure(move |_, args, context| {
            let realm = realm_of(&w)?;
            let id = args.first().cloned().unwrap_or_default().to_u32(context)? as usize;
            let func = realm.funcs.borrow().get(id).copied().ok_or_else(|| JsNativeError::typ().with_message("no such function"))?;
            let list = args.get(1).and_then(JsValue::as_object).ok_or_else(JsNativeError::typ)?;
            let list = JsArray::from_object(list)?;
            let mut call_args = Vec::new();
            for i in 0..list.length(context)? {
                call_args.push(list.get(i, context)?);
            }
            call_func(&realm, func, &call_args, context)
        })
    };

    let w = weak.clone();
    // memoryNew(initial, maximum | -1) -> [memId, buffer]
    let memory_new = unsafe {
        NativeFunction::from_closure(move |_, args, context| {
            let realm = realm_of(&w)?;
            let initial = args.first().cloned().unwrap_or_default().to_u32(context)?;
            let maximum = args.get(1).cloned().unwrap_or_default().to_number(context)?;
            let maximum = (maximum >= 0.0).then_some(maximum as u32);
            let len = (initial as usize).checked_mul(PAGE).ok_or_else(|| JsNativeError::range().with_message("WebAssembly.Memory(): could not allocate memory"))?;
            if realm.memory_bytes.get().saturating_add(len) > MAX_REALM_MEMORY {
                return Err(JsNativeError::range().with_message("WebAssembly.Memory(): could not allocate memory").into());
            }
            let mut block = boa_engine::object::builtins::AlignedVec::<u8>::new(64);
            block
                .try_reserve_exact(len)
                .map_err(|_| JsNativeError::range().with_message("WebAssembly.Memory(): could not allocate memory"))?;
            block.resize(len, 0);
            // A detach key nothing outside this crate has: script cannot
            // detach the buffer (`transfer()` is a TypeError), and the engine
            // never does while the store lives.
            let key = JsValue::from(boa_engine::JsSymbol::new(Some(js_string!("wasm memory"))).ok_or_else(JsNativeError::typ)?);
            let buffer = JsArrayBuffer::from_byte_block_with_detach_key(block, key, context)?;
            let (ptr, size) = {
                let mut data = buffer.data_mut().ok_or_else(JsNativeError::typ)?;
                (data.as_mut_ptr(), data.len())
            };
            // SAFETY (the `&'static mut` invariant, docs/design/WASM-W1.md):
            // the slice is the `ArrayBuffer`'s storage, which never moves or
            // frees while this realm's store lives: the realm holds the buffer
            // object in `memories` (dropped with the store), the detach key
            // keeps script from taking the storage, and the buffer is not
            // resizable. Only the store reads or writes through it.
            let bytes: &'static mut [u8] = unsafe { std::slice::from_raw_parts_mut(ptr, size) };
            let ty = wasmi::MemoryType::new(initial, maximum);
            let memory = realm
                .with_store(|s| wasmi::Memory::new_static(s.ctx(), ty, bytes))
                .map_err(|e| JsNativeError::range().with_message(format!("WebAssembly.Memory(): {e}")))?;
            realm.memory_bytes.set(realm.memory_bytes.get() + len);
            let buffer_object: boa_engine::JsObject = buffer.into();
            let mut memories = realm.memories.borrow_mut();
            memories.push((memory, Some(buffer_object.clone())));
            let row: [JsValue; 2] = [JsValue::from((memories.len() - 1) as u32), buffer_object.into()];
            Ok(JsArray::from_iter(row, context).into())
        })
    };
    let w = weak.clone();
    // memoryGrow(memId, delta) -> old pages. W1: only grow(0) (docs/design/WASM-W1.md).
    let memory_grow = unsafe {
        NativeFunction::from_closure(move |_, args, context| {
            let realm = realm_of(&w)?;
            let id = args.first().cloned().unwrap_or_default().to_u32(context)? as usize;
            let delta = args.get(1).cloned().unwrap_or_default().to_u32(context)?;
            let memory = realm.memories.borrow().get(id).map(|m| m.0).ok_or_else(JsNativeError::typ)?;
            let pages = realm.with_store(|s| memory.size(s.ctx()));
            if delta == 0 {
                return Ok(JsValue::from(pages as u32));
            }
            Err(JsNativeError::range()
                .with_message("WebAssembly.Memory.grow(): Unable to grow instance memory (WASM W1: no grow; W2)")
                .into())
        })
    };

    let w = weak.clone();
    // tableNew(initial, maximum | -1) -> tableId (funcref, null-filled)
    let table_new = unsafe {
        NativeFunction::from_closure(move |_, args, context| {
            let realm = realm_of(&w)?;
            let initial = args.first().cloned().unwrap_or_default().to_u32(context)?;
            let maximum = args.get(1).cloned().unwrap_or_default().to_number(context)?;
            let maximum = (maximum >= 0.0).then_some(maximum as u32);
            let ty = wasmi::TableType::new(wasmi::RefType::Func, initial, maximum);
            let table = realm
                .with_store(|s| wasmi::Table::new(s.ctx(), ty, wasmi::Ref::Func(wasmi::Nullable::Null)))
                .map_err(|e| JsNativeError::range().with_message(format!("WebAssembly.Table(): {e}")))?;
            let mut tables = realm.tables.borrow_mut();
            tables.push(table);
            Ok(JsValue::from((tables.len() - 1) as u32))
        })
    };
    let w = weak.clone();
    // tableOp(tableId, op, a, b): "length" | "get" i | "set" i funcId | "grow" delta
    let table_op = unsafe {
        NativeFunction::from_closure(move |_, args, context| {
            let realm = realm_of(&w)?;
            let id = args.first().cloned().unwrap_or_default().to_u32(context)? as usize;
            let op = args.get(1).cloned().unwrap_or_default().to_string(context)?.to_std_string_escaped();
            let table = realm.tables.borrow().get(id).copied().ok_or_else(JsNativeError::typ)?;
            let size = realm.with_store(|s| table.size(s.ctx()));
            match op.as_str() {
                "length" => Ok(JsValue::from(size as u32)),
                "get" => {
                    let i = args.get(2).cloned().unwrap_or_default().to_u32(context)? as u64;
                    if i >= size {
                        return Err(JsNativeError::range().with_message("WebAssembly.Table.get(): invalid address").into());
                    }
                    let r = realm.with_store(|s| table.get(s.ctx(), i)).ok_or_else(JsNativeError::typ)?;
                    funcref_to_js(&realm, r, context)
                }
                "set" => {
                    let i = args.get(2).cloned().unwrap_or_default().to_u32(context)? as u64;
                    if i >= size {
                        return Err(JsNativeError::range().with_message("WebAssembly.Table.set(): invalid address").into());
                    }
                    let r = funcref_from_js(&realm, &args.get(3).cloned().unwrap_or_default())?;
                    realm
                        .with_store(|s| table.set(s.ctx(), i, r))
                        .map_err(|e| JsNativeError::typ().with_message(format!("WebAssembly.Table.set(): {e}")))?;
                    Ok(JsValue::undefined())
                }
                "grow" => {
                    let delta = args.get(2).cloned().unwrap_or_default().to_u32(context)? as u64;
                    realm
                        .with_store(|s| table.grow(s.ctx(), delta, wasmi::Ref::Func(wasmi::Nullable::Null)))
                        .map(|old| JsValue::from(old as u32))
                        .map_err(|_| JsNativeError::range().with_message("WebAssembly.Table.grow(): failed to grow table").into())
                }
                _ => Err(JsNativeError::typ().into()),
            }
        })
    };
    let w = weak.clone();
    // globalNew(type, mutable, value) -> globalId
    let global_new = unsafe {
        NativeFunction::from_closure(move |_, args, context| {
            let realm = realm_of(&w)?;
            let ty = args.first().cloned().unwrap_or_default().to_string(context)?.to_std_string_escaped();
            let ty = val_type(&ty).ok_or_else(|| JsNativeError::typ().with_message("WebAssembly.Global(): value type"))?;
            let mutability = if args.get(1).is_some_and(JsValue::to_boolean) { wasmi::Mutability::Var } else { wasmi::Mutability::Const };
            let value = match args.get(2) {
                Some(v) if !v.is_undefined() => to_wasm(v, ty, context)?,
                _ => wasmi::Val::default_for_ty(ty),
            };
            let global = realm.with_store(|s| wasmi::Global::new(s.ctx(), value, mutability));
            let mut globals = realm.globals.borrow_mut();
            globals.push(global);
            Ok(JsValue::from((globals.len() - 1) as u32))
        })
    };
    let w = weak.clone();
    // globalOp(globalId, "get" | "set", value)
    let global_op = unsafe {
        NativeFunction::from_closure(move |_, args, context| {
            let realm = realm_of(&w)?;
            let id = args.first().cloned().unwrap_or_default().to_u32(context)? as usize;
            let op = args.get(1).cloned().unwrap_or_default().to_string(context)?.to_std_string_escaped();
            let global = realm.globals.borrow().get(id).copied().ok_or_else(JsNativeError::typ)?;
            let ty = realm.with_store(|s| global.ty(s.ctx()));
            if op == "get" {
                let v = realm.with_store(|s| global.get(s.ctx()));
                return to_js(&v);
            }
            if !ty.mutability().is_mut() {
                return Err(JsNativeError::typ().with_message("WebAssembly.Global.value: Can't set the value of an immutable global.").into());
            }
            let v = to_wasm(&args.get(2).cloned().unwrap_or_default(), ty.content(), context)?;
            realm.with_store(|s| global.set(s.ctx(), v)).map_err(|e| JsNativeError::typ().with_message(e.to_string()))?;
            Ok(JsValue::undefined())
        })
    };

    Ok(boa_engine::object::ObjectInitializer::new(context)
        .function(table_new, js_string!("tableNew"), 2)
        .function(table_op, js_string!("tableOp"), 4)
        .function(global_new, js_string!("globalNew"), 3)
        .function(global_op, js_string!("globalOp"), 3)
        .function(memory_new, js_string!("memoryNew"), 2)
        .function(memory_grow, js_string!("memoryGrow"), 2)
        .function(validate, js_string!("validate"), 1)
        .function(compile, js_string!("compile"), 1)
        .function(reflect, js_string!("reflect"), 2)
        .function(instantiate, js_string!("instantiate"), 2)
        .function(call, js_string!("call"), 2)
        .function(set_convert, js_string!("setConvert"), 1)
        .function(make_export, js_string!("makeExport"), 3)
        .build())
}

#[cfg(test)]
mod tests {
    use super::export_func_indices;

    const HEADER: [u8; 8] = [0, 97, 115, 109, 1, 0, 0, 0];

    fn with_sections(sections: &[(u8, &[u8])]) -> Vec<u8> {
        let mut out = HEADER.to_vec();
        for (id, body) in sections {
            out.push(*id);
            out.push(body.len() as u8);
            out.extend_from_slice(body);
        }
        out
    }

    #[test]
    fn no_export_section_gives_nothing() {
        assert!(export_func_indices(&with_sections(&[(1, &[1, 0x60, 0, 0])])).is_empty());
    }

    #[test]
    fn a_function_export_and_a_multi_byte_index() {
        // two exports: "a" -> func 3; "bb" -> func 200 (LEB 0xc8 0x01); a memory export skipped
        let exports: &[u8] = &[3, 1, b'a', 0, 3, 2, b'b', b'b', 0, 0xc8, 0x01, 1, b'm', 2, 0];
        let got = export_func_indices(&with_sections(&[(7, exports)]));
        assert_eq!(got.get("a"), Some(&3));
        assert_eq!(got.get("bb"), Some(&200));
        assert_eq!(got.get("m"), None);
    }

    #[test]
    fn a_truncated_section_stops_cleanly() {
        let mut bytes = with_sections(&[(7, &[1, 5, b'a'])]);
        bytes.truncate(bytes.len() - 1);
        assert!(export_func_indices(&bytes).is_empty());
        // A section size past the end.
        let mut lying = HEADER.to_vec();
        lying.extend_from_slice(&[7, 0x7f, 1]);
        assert!(export_func_indices(&lying).is_empty());
    }
}
