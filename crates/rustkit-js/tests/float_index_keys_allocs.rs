//! JS-PERF1: a whole-number double used as `obj[n]` must not allocate a string for its key.
//! Profiled on CodeMirror's first parse (3,404 engine samples): number keys turned into strings
//! and parsed back were ~7%. Counting allocations is deterministic where a timing is not; this is
//! the only test in its binary, so nothing else allocates while it counts.

use rustkit_js::JsRuntime;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};

struct Counting;
static ALLOCS: AtomicU64 = AtomicU64::new(0);
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        unsafe { System.realloc(p, l, n) }
    }
}
#[global_allocator]
static A: Counting = Counting;

#[test]
fn float_keys_cost_about_what_integer_keys_cost() {
    let mut rt = JsRuntime::new().unwrap();
    rt.evaluate_script("var a = []; for (var i = 0; i < 1000; i++) a.push(i);").unwrap();
    let mut per_read = |idx: &str| -> f64 {
        let src = format!("(function(){{ var s = 0; for (var i = 0; i < 1000; i++) {{ s += a[{idx}]; }} return s; }})()");
        rt.evaluate_script(&src).unwrap(); // warm up
        let before = ALLOCS.load(Ordering::Relaxed);
        for _ in 0..20 {
            rt.evaluate_script(&src).unwrap();
        }
        (ALLOCS.load(Ordering::Relaxed) - before) as f64 / 20_000.0
    };
    let int = per_read("i");
    let float_whole = per_read("(i + 0.5) - 0.5");
    let floor = per_read("Math.floor(i / 2)");
    let sqrt = per_read("Math.sqrt(i * i)");
    println!("allocs per read: int {int:.2}, float {float_whole:.2}, floor {floor:.2}, sqrt {sqrt:.2}");
    // Integer keys allocate ~0.1 per read here (the loop itself). A float key used to add one or
    // two more (the string and its parse); allow the integer cost plus a little noise.
    for (name, v) in [("float", float_whole), ("Math.floor", floor), ("Math.sqrt", sqrt)] {
        assert!(v <= int + 0.3, "{name} key: {v:.2} allocations per read, integer key {int:.2}");
    }
}
