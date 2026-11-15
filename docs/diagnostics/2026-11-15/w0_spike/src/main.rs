//! W0 spike (scratch, not committed): can one Boa ArrayBuffer's storage back a wasmi memory, survive
//! memory.grow, and stay valid while an imported host function reads it?
use aligned_vec::{AVec, ConstAlign};
use boa_engine::{js_string, object::builtins::JsArrayBuffer, Context, JsValue, Source};
use std::sync::Arc;
use wasmi::*;

const PAGE: usize = 65536;
type AVecU8 = AVec<u8, ConstAlign<64>>;

/// A host view of the shared storage that wasm's imported function reads through (the "Boa side").
#[derive(Clone)]
struct HostView(Arc<std::sync::Mutex<*const u8>>);
unsafe impl Send for HostView {}
unsafe impl Sync for HostView {}
impl HostView { fn get(&self) -> *const u8 { *self.0.lock().unwrap() } }

fn main() {
    let wat = r#"(module
      (import "env" "memory" (memory 1 4))
      (import "env" "host_read" (func $host_read (param i32) (result i32)))
      (func (export "store32") (param i32 i32) local.get 0 local.get 1 i32.store)
      (func (export "load32") (param i32) (result i32) local.get 0 i32.load)
      (func (export "grow") (param i32) (result i32) local.get 0 memory.grow)
      (func (export "size") (result i32) memory.size)
      (func (export "call_host") (param i32) (result i32) local.get 0 call $host_read))"#;
    let wasm = wat::parse_str(wat).unwrap();

    let mut ctx = Context::default();
    // ---- reserve the memory's MAXIMUM (4 pages) up front; the ArrayBuffer's length is the current size (1 page).
    let (init_pages, max_pages) = (1usize, 4usize);
    let mut vec: AVecU8 = AVec::with_capacity(64, max_pages * PAGE);
    let ptr = vec.as_mut_ptr();
    // initialise the whole reservation (spare capacity must not be handed out as &mut [u8] uninitialised)
    unsafe { std::ptr::write_bytes(ptr, 0, max_pages * PAGE) };
    vec.resize(init_pages * PAGE, 0);
    assert_eq!(vec.as_mut_ptr(), ptr, "resize within capacity must not move the storage");
    let ab = JsArrayBuffer::from_byte_block(vec, &mut ctx).unwrap();
    println!("[1] ArrayBuffer byteLength = {}", ab.byte_length());

    // ---- a wasmi memory over the SAME storage, reserved to the maximum.
    let engine = Engine::default();
    let mut store = Store::new(&engine, ());
    let slice: &'static mut [u8] = unsafe { std::slice::from_raw_parts_mut(ptr, max_pages * PAGE) };
    let ty = MemoryType::new(init_pages as u32, Some(max_pages as u32));
    let mem = Memory::new_static(&mut store, ty, slice).unwrap();
    println!("[2] wasmi Memory::new_static over the buffer: ok, size = {} bytes, data ptr same as the ArrayBuffer's: {}", mem.data_size(&store), mem.data_ptr(&store) as *const u8 == ptr as *const u8);

    // ---- an imported host function that reads the memory through a pointer to the storage while wasm is mid-call.
    let view = HostView(Arc::new(std::sync::Mutex::new(ptr as *const u8)));
    let v2 = view.clone();
    let mut linker = <Linker<()>>::new(&engine);
    linker.define("env", "memory", mem).unwrap();
    linker
        .func_wrap("env", "host_read", move |a: i32| -> i32 {
            let p = v2.get();
            unsafe { std::ptr::read_unaligned(p.add(a as usize) as *const i32) }
        })
        .unwrap();
    let module = Module::new(&engine, &wasm[..]).unwrap();
    let inst = linker.instantiate_and_start(&mut store, &module).unwrap();
    let load32 = inst.get_typed_func::<i32, i32>(&store, "load32").unwrap();
    let store32 = inst.get_typed_func::<(i32, i32), ()>(&store, "store32").unwrap();
    let grow = inst.get_typed_func::<i32, i32>(&store, "grow").unwrap();
    let size = inst.get_typed_func::<(), i32>(&store, "size").unwrap();
    let call_host = inst.get_typed_func::<i32, i32>(&store, "call_host").unwrap();

    // T1: a write through the ArrayBuffer is visible to wasm
    ab.data_mut().unwrap()[..4].copy_from_slice(&[1, 2, 3, 4]);
    let got = load32.call(&mut store, 0).unwrap();
    println!("[3] JS-side write [1,2,3,4] -> wasm load32(0) = {:#x} (want 0x4030201): {}", got, got == 0x0403_0201);
    // T2: a wasm store is visible through the ArrayBuffer
    store32.call(&mut store, (8, 0xdeadbeefu32 as i32)).unwrap();
    let seen = u32::from_le_bytes(ab.data().unwrap()[8..12].try_into().unwrap());
    println!("[4] wasm store32(8, 0xdeadbeef) -> ArrayBuffer bytes 8..12 = {:#x}: {}", seen, seen == 0xdeadbeef);
    // T3: the imported function reads the same bytes mid-call
    let via_host = call_host.call(&mut store, 8).unwrap();
    println!("[5] wasm -> imported host fn reads storage mid-call: {:#x}: {}", via_host as u32, via_host as u32 == 0xdeadbeef);

    // T4: memory.grow in place; the old ArrayBuffer is detached and a new one over the same storage is made (what the spec requires).
    let before_ptr = mem.data_ptr(&store) as *const u8;
    let prev = grow.call(&mut store, 1).unwrap();
    let wasm_size = size.call(&mut store, ()).unwrap();
    let new_len = mem.data_size(&store);
    let after_ptr = mem.data_ptr(&store) as *const u8;
    println!("[6] wasm memory.grow(1): returned {prev}, size {wasm_size} pages, data_size {new_len}, storage pointer unchanged: {}", before_ptr == after_ptr);
    let mut old = ab.detach(&JsValue::undefined()).unwrap();
    println!("[7] old ArrayBuffer detached: byteLength now {} ; storage taken back as an AlignedVec of len {}", ab.byte_length(), old.len());
    assert_eq!(old.as_mut_ptr(), ptr);
    old.resize(new_len, 0);
    assert_eq!(old.as_mut_ptr(), ptr, "resize within the reservation must not move the storage");
    let ab2 = JsArrayBuffer::from_byte_block(old, &mut ctx).unwrap();
    let keep = ab2.data().unwrap()[..4].to_vec();
    let keep2 = u32::from_le_bytes(ab2.data().unwrap()[8..12].try_into().unwrap());
    println!("[8] new ArrayBuffer byteLength = {}, old contents kept: {:?} {:#x}", ab2.byte_length(), keep, keep2);
    // beyond the old size, through wasm and through the new buffer
    store32.call(&mut store, (PAGE as i32 + 16, 0x1234_5678)).unwrap();
    let hi = u32::from_le_bytes(ab2.data().unwrap()[PAGE + 16..PAGE + 20].try_into().unwrap());
    println!("[9] wasm store in the new page -> new ArrayBuffer sees {:#x}: {}", hi, hi == 0x1234_5678);
    // grow past the maximum fails the way the spec says (-1)
    let over = grow.call(&mut store, 10).unwrap();
    println!("[10] grow past max (10 pages): returned {over} (want -1): {}", over == -1);

    // T5: can script detach/transfer the buffer? (it must not be able to, for a wasm memory)
    ctx.global_object().set(js_string!("wbuf"), ab2.clone(), true, &mut ctx).unwrap();
    let r = ctx.eval(Source::from_bytes("try { wbuf.transfer(); 'script CAN transfer it (needs a detach key)' } catch (e) { 'script cannot: ' + e.name }"));
    println!("[11] script `wbuf.transfer()` on a buffer made by from_byte_block: {:?}", r.map(|v| v.display().to_string()));
}
