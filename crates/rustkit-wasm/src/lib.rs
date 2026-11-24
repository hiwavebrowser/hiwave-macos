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
}

/// One realm's WebAssembly state.
pub struct WasmRealm {
    id: u64,
    engine: wasmi::Engine,
    store: RefCell<wasmi::Store<HostData>>,
    /// Compiled modules, with each exported function's index in the module's
    /// function space (an exported function's JS `name`, as in Chrome).
    modules: RefCell<Vec<(wasmi::Module, HashMap<String, u32>)>>,
    funcs: RefCell<Vec<wasmi::Func>>,
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
        let data = HostData { realm: id, imports: Vec::new(), contexts: Vec::new(), pending: None };
        let store = wasmi::Store::new(&engine, data);
        Rc::new(Self { id, engine, store: RefCell::new(store), modules: RefCell::default(), funcs: RefCell::default() })
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

/// Each exported function's index, from the module's export section (id 7).
/// The bytes have already been validated.
fn export_func_indices(bytes: &[u8]) -> HashMap<String, u32> {
    fn leb(b: &[u8], i: &mut usize) -> Option<u32> {
        let (mut r, mut shift) = (0u32, 0);
        loop {
            let x = *b.get(*i)?;
            *i += 1;
            r |= u32::from(x & 0x7f).checked_shl(shift)?;
            if x & 0x80 == 0 {
                return Some(r);
            }
            shift += 7;
        }
    }
    let mut out = HashMap::new();
    let mut i = 8;
    while i < bytes.len() {
        let id = bytes[i];
        i += 1;
        let Some(size) = leb(bytes, &mut i) else { break };
        let end = i + size as usize;
        if id == 7 {
            let mut j = i;
            let n = leb(bytes, &mut j).unwrap_or(0);
            for _ in 0..n {
                let Some(len) = leb(bytes, &mut j) else { break };
                let name = String::from_utf8_lossy(bytes.get(j..j + len as usize).unwrap_or_default()).into_owned();
                j += len as usize;
                let kind = bytes.get(j).copied().unwrap_or(0xff);
                j += 1;
                let Some(index) = leb(bytes, &mut j) else { break };
                if kind == 0 {
                    out.insert(name, index);
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
            let linked: JsResult<Vec<(wasmi::Func, String, usize, u32)>> = realm.with_store(|s| {
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
                        _ => {
                            return Err(wasm_error(
                                "LinkError",
                                format!(
                                    "Import \"{}\" \"{}\": memory, table and global imports arrive in W1 phases 2-3",
                                    key.0, key.1
                                ),
                            ))
                        }
                    }
                }
                let instance = linker
                    .instantiate_and_start(&mut ctx, &module)
                    .map_err(|e| match e.as_trap_code() {
                        Some(code) => wasm_error("RuntimeError", trap_message(code)),
                        None => wasm_error("LinkError", e),
                    })?;
                let mut out = Vec::new();
                for export in module.exports() {
                    if let Some(func) = instance.get_func(ctx.as_context(), export.name()) {
                        let arity = func.ty(ctx.as_context()).params().len();
                        let index = indices.get(export.name()).copied().unwrap_or(0);
                        out.push((func, export.name().to_string(), arity, index));
                    }
                }
                Ok(out)
            });
            let mut funcs = realm.funcs.borrow_mut();
            let mut rows = Vec::new();
            for (func, name, arity, index) in linked? {
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

    Ok(boa_engine::object::ObjectInitializer::new(context)
        .function(validate, js_string!("validate"), 1)
        .function(compile, js_string!("compile"), 1)
        .function(reflect, js_string!("reflect"), 2)
        .function(instantiate, js_string!("instantiate"), 2)
        .function(call, js_string!("call"), 2)
        .build())
}
