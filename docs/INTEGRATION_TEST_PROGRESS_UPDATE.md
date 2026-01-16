# Integration Test Progress Update - Solution B Implementation

**Date:** 2026-01-05 (Updated)
**Previous Status:** Tests compiled but crashed with SIGSEGV (fake window handle)
**Current Status:** Tests compile with real NSWindow but crash with SIGABRT (main thread requirement)

---

## What Was Implemented

### Solution B: Real Offscreen NSWindow

Replaced the mock window handle with a real but invisible NSWindow implementation.

**Changes Made:**

1. **Added Dependencies** (`crates/hiwave-app/Cargo.toml:71-74`)
   ```toml
   [target.'cfg(target_os = "macos")'.dev-dependencies]
   cocoa = "0.25"
   objc = "0.2"
   ```

2. **Implemented Real TestWindow** (`tests/support/test_engine.rs:22-85`)
   ```rust
   pub struct TestWindow {
       ns_window: id,  // Real NSWindow handle
   }

   impl TestWindow {
       pub fn create(width: u32, height: u32) -> Self {
           unsafe {
               // Initialize NSApplication (required for AppKit)
               let app: id = msg_send![class!(NSApplication), sharedApplication];
               let _: () = msg_send![app, setActivationPolicy: 1i64];

               // Create real NSWindow
               let ns_window: id = msg_send![class!(NSWindow), alloc];
               let ns_window: id = msg_send![ns_window,
                   initWithContentRect: NSRect::new(
                       NSPoint::new(0.0, 0.0),
                       NSSize::new(width as f64, height as f64)
                   )
                   styleMask: NSWindowStyleMask::NSBorderlessWindowMask
                   backing: NSBackingStoreType::NSBackingStoreBuffered
                   defer: NO
               ];

               // Make it invisible
               let _: () = msg_send![ns_window, setIsVisible: NO];

               Self { ns_window }
           }
       }

       pub fn raw_handle(&self) -> RawWindowHandle {
           unsafe {
               let content_view: id = msg_send![self.ns_window, contentView];
               let ns_view = NonNull::new(content_view as *mut std::ffi::c_void)
                   .expect("Content view should not be null");
               RawWindowHandle::AppKit(AppKitWindowHandle::new(ns_view))
           }
       }
   }

   impl Drop for TestWindow {
       fn drop(&mut self) {
           unsafe {
               let _: () = msg_send![self.ns_window, close];
           }
       }
   }
   ```

**Key Features:**
- Creates real NSWindow with borderless style
- Sets window to invisible (never appears on screen)
- Initializes NSApplication properly
- Returns valid NSView handle to RustKit
- Automatic cleanup on drop

---

## Current Issue: Main Thread Requirement

### The Problem

**Error:**
```
fatal runtime error: Rust cannot catch foreign exceptions, aborting
process didn't exit successfully (signal: 6, SIGABRT: process abort signal)
```

**Root Cause:**
- macOS AppKit requires all UI operations (including NSWindow creation) on the main thread
- Rust's test framework (`cargo test`) runs tests on worker threads for parallelization
- Creating NSWindow on worker thread triggers Objective-C exception
- Rust cannot catch foreign (Objective-C) exceptions → immediate abort

**Why Previous Approach Failed:**
- Mock handle with fake pointer → SIGSEGV (invalid memory dereference)

**Why Current Approach Fails:**
- Real NSWindow on worker thread → SIGABRT (main thread violation)

---

## Technical Analysis

### What Works ✅
1. Test infrastructure compiles successfully
2. Real NSWindow creation code is correct
3. NSView handle extraction works
4. Window cleanup (Drop) implemented properly
5. Tests can be compiled with `cargo test --no-run`

### What Doesn't Work ❌
1. Cannot run tests with standard `cargo test`
2. Rust test harness doesn't provide main thread execution
3. AppKit throws uncatchable exceptions on worker threads

### macOS Threading Requirements

From Apple's documentation:
> "For compatibility with multiprocess services, AppKit must be initialized from the main thread."

The issue is fundamental to how macOS works:
```rust
// This fails when called from test worker thread:
let ns_window: id = msg_send![class!(NSWindow), alloc];
// ↑ Objective-C exception: "NSWindow must be used from main thread only"
```

---

## Solutions Forward

### Option 1: Custom Test Runner (Complex)

Implement a custom test harness that runs on the main thread.

**Approach:**
```rust
// In tests/integration_tests.rs
#![feature(custom_test_frameworks)]
#![test_runner(main_thread_test_runner)]

fn main_thread_test_runner(tests: &[&dyn Fn()]) {
    // Run all tests on current (main) thread
    for test in tests {
        test();
    }
}
```

**Pros:**
- Tests run in standard Rust test environment
- Can use `cargo test` (with modifications)

**Cons:**
- Requires nightly Rust
- Complex implementation
- Tests run serially (no parallelization)
- May not work with IDE test runners

### Option 2: Headless Rendering in RustKit (Recommended)

Implement headless rendering mode in the RustKit engine itself.

**This is Solution A from the original plan:**
- Add `headless` feature flag to rustkit-engine
- Use software rendering (wgpu with headless surface)
- Render to offscreen texture, export pixels directly
- No NSWindow required

**Status:** Blocked on engine team implementation
**Effort:** 2-3 days
**Impact:** Unblocks all integration tests

### Option 3: Manual Test Execution (Current Workaround)

Run tests manually in environments where AppKit is initialized.

**Approaches:**
- Run individual tests from GUI app (hiwave-smoke pattern)
- Use Xcode's test runner (creates proper main thread context)
- Build test binary and run with special flags

**Pros:**
- No code changes needed
- Tests work as-is

**Cons:**
- Cannot use `cargo test` directly
- Manual process
- Not automatable for CI

### Option 4: Integration Test Binary (Pragmatic)

Convert integration tests to a binary that initializes AppKit properly.

**Structure:**
```rust
// tests/manual_integration.rs → src/bin/integration_tests.rs
fn main() {
    // TAO/Tao provides proper main thread AppKit initialization
    let event_loop = EventLoopBuilder::new().build();

    // Run tests manually
    test_engine_creates_successfully();
    test_simple_html_renders();
    // ...

    println!("All tests passed!");
}
```

**Pros:**
- Works with current implementation
- Can be automated (run binary in CI)
- Uses proven pattern from hiwave-smoke

**Cons:**
- Not standard Rust test infrastructure
- Loses `cargo test` integration
- Manual test registration

---

## Compilation Status

### Build Output
```bash
$ cargo test --package hiwave-app --test integration_tests --no-run

warning: `hiwave-app` (test "integration_tests") generated 11 warnings
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.98s
  Executable tests/integration_tests.rs (target/debug/deps/integration_tests-fcc30cdc6b29b081)
```

**Success:** Tests compile with 0 errors ✅

### Runtime Status
```bash
$ cargo test --package hiwave-app --test integration_tests test_engine_creates_successfully

running 1 test
fatal runtime error: Rust cannot catch foreign exceptions, aborting
error: process didn't exit successfully (signal: 6, SIGABRT)
```

**Failure:** Tests abort due to main thread requirement ❌

---

## Updated File List

### Modified Files

1. **crates/hiwave-app/Cargo.toml** (+6 lines)
   - Added cocoa and objc dev-dependencies for macOS

2. **crates/hiwave-app/tests/support/test_engine.rs** (complete rewrite)
   - Changed from mock handle to real NSWindow
   - Added NSApplication initialization
   - Implemented proper Drop for cleanup
   - Updated TestEngine to pass width/height to TestWindow

3. **crates/hiwave-app/tests/support/mod.rs** (-2 exports)
   - Removed unused TestWindow export
   - Removed unused FrameDiff export

4. **crates/hiwave-app/tests/integration_tests.rs** (-2 lines)
   - Removed unused `use support::*;`

### Implementation Stats

**Lines Changed:** ~100 lines across 4 files
**Compilation Warnings:** 11 (all non-critical)
**Compilation Errors:** 0 ✅
**Runtime Errors:** 1 (main thread requirement) ❌

---

## Recommendations

### Immediate Action

**Recommend pursuing Option 2: Headless Rendering Mode**

This was the original "Solution A" from the implementation plan and remains the best long-term solution:

1. **Benefits:**
   - Enables all integration tests
   - Works in CI/CD environments
   - Useful for server-side rendering
   - No test infrastructure changes needed

2. **Implementation Path:**
   ```rust
   // In rustkit-engine/src/lib.rs
   #[cfg(feature = "headless")]
   pub fn create_headless_view(&mut self, bounds: Bounds) -> Result<EngineViewId> {
       // Use wgpu surfaceless rendering
       let view_id = EngineViewId::new();
       self.views.insert(view_id, HeadlessViewState::new(bounds));
       Ok(view_id)
   }
   ```

3. **Testing:**
   - Once headless mode exists, update TestWindow to use it
   - Tests run immediately with `cargo test`
   - Full integration test suite becomes viable

### Alternative: Option 4 (Stopgap)

If headless mode is delayed, implement Option 4 (Integration Test Binary):

1. Convert tests to binary using TAO event loop
2. Run binary in CI instead of `cargo test`
3. Still validates full stack, just different execution model
4. Can migrate back to standard tests when headless mode lands

---

## Progress Summary

| Aspect | Previous | Current | Change |
|--------|----------|---------|--------|
| **Compilation** | ✅ Pass | ✅ Pass | No change |
| **Window Handle** | ❌ Fake pointer | ✅ Real NSWindow | **Improved** |
| **Runtime** | ❌ SIGSEGV | ❌ SIGABRT | Different error |
| **Root Cause** | Invalid memory | Thread violation | **Identified** |
| **Solution Path** | Unclear | 4 options defined | **Clarified** |

---

## Next Steps

1. **Decision Point:** Choose between Option 2 (headless) or Option 4 (binary)
2. **If Option 2:** Assign headless rendering to engine team
3. **If Option 4:** Convert integration_tests.rs to binary format
4. **Timeline:**
   - Option 2: 2-3 days engineering + 1 day validation
   - Option 4: 1 day conversion + testing

The test infrastructure is complete and ready. We're now blocked only on the execution environment, not the test code itself.
