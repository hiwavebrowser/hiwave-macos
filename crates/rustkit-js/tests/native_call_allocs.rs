//! JS-PERF2 item 3: a call to a native function (`Math.floor(x)`, `parseInt(s)`, `s.charCodeAt(i)`)
//! must not allocate for its argument list. `native_function_call` popped the arguments into a
//! `Vec`: one malloc and free per call, measured as exactly one allocation per call against none
//! for a call between script functions. Counting allocations is deterministic where a timing is
//! not; this is the only test in its binary, so nothing else allocates while it counts.

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
fn native_calls_do_not_allocate_for_their_arguments() {
    let mut rt = JsRuntime::new().unwrap();
    rt.evaluate_script("var str = 'abcdefghij'.repeat(100); function id(x) { return x; }").unwrap();
    let mut per_iter = |body: &str| -> f64 {
        let src = format!("(function(){{ var s = 0; for (var i = 0; i < 1000; i++) {{ {body} }} return s; }})()");
        rt.evaluate_script(&src).unwrap(); // warm up
        let before = ALLOCS.load(Ordering::Relaxed);
        for _ in 0..20 {
            rt.evaluate_script(&src).unwrap();
        }
        (ALLOCS.load(Ordering::Relaxed) - before) as f64 / 20_000.0
    };
    let baseline = per_iter("s += id(i);"); // a script-to-script call
    let floor = per_iter("s += Math.floor(i / 2);");
    let max2 = per_iter("s += Math.max(i, 3);");
    let char_code = per_iter("s += str.charCodeAt(i);");
    println!("allocs per iteration: script call {baseline:.2}, floor {floor:.2}, max {max2:.2}, charCodeAt {char_code:.2}");
    for (name, v) in [("Math.floor", floor), ("Math.max", max2), ("charCodeAt", char_code)] {
        assert!(v <= baseline + 0.3, "{name}: {v:.2} allocations per call, a script call {baseline:.2}");
    }
}
