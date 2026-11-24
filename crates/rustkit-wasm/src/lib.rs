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
        // Tables are phase 3; until then a table keeps its initial size.
        Ok(current == 0 || desired <= current)
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
}

impl WasmRealm {
    pub fn new() -> Rc<Self> {
        // Stated W1 gaps (docs/design/WASM-W1.md): no memory64, no SIMD. Both
        // are left out at build time (wasmi without its `memory64` and `simd`
        // features), so a module using either fails validation.
        let config = wasmi::Config::default();
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
        TrapCode::IndirectCallToNull | TrapCode::BadSignature => "null function or function signature mismatch",
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
        ctx.data_mut().contexts.push(context as *mut Context);
        let r = func.call(&mut ctx, &params, &mut results);
        ctx.data_mut().contexts.pop();
        r.map_err(|e| (Some(e), ctx.data_mut().pending.take()))
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
            let linked: JsResult<(Vec<(wasmi::Func, String, usize, u32)>, Vec<(String, wasmi::Memory)>)> = realm.with_store(|s| {
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
                        _ => {
                            return Err(wasm_error(
                                "LinkError",
                                format!("Import \"{}\" \"{}\": table and global imports arrive in W1 phase 3", key.0, key.1),
                            ))
                        }
                    }
                }
                ctx.data_mut().contexts.push(context_ptr);
                let started = linker.instantiate_and_start(&mut ctx, &module);
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
                for export in module.exports() {
                    if let Some(func) = instance.get_func(ctx.as_context(), export.name()) {
                        let arity = func.ty(ctx.as_context()).params().len();
                        let index = indices.get(export.name()).copied().unwrap_or(0);
                        out.push((func, export.name().to_string(), arity, index));
                    } else if let Some(memory) = instance.get_memory(ctx.as_context(), export.name()) {
                        memories.push((export.name().to_string(), memory));
                    }
                }
                Ok((out, memories))
            });
            let (linked, exported_memories) = linked?;
            let mut rows = Vec::new();
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
            let mut funcs = realm.funcs.borrow_mut();
            for (func, name, arity, index) in linked {
                funcs.push(func);
                let row: [JsValue; 4] = [
                    js_string!(name).into(),
                    JsValue::from((funcs.len() - 1) as u32),
                    JsValue::from(arity as u32),
                    JsValue::from(index),
                ];
                rows.push(JsArray::from_iter(row, context).into());
            }
            Ok(JsArray::from_iter(rows, context).into())
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

    Ok(boa_engine::object::ObjectInitializer::new(context)
        .function(memory_new, js_string!("memoryNew"), 2)
        .function(memory_grow, js_string!("memoryGrow"), 2)
        .function(validate, js_string!("validate"), 1)
        .function(compile, js_string!("compile"), 1)
        .function(reflect, js_string!("reflect"), 2)
        .function(instantiate, js_string!("instantiate"), 2)
        .function(call, js_string!("call"), 2)
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
