//! Call-depth limits for page script, as close to a browser's as the thread
//! script runs on allows.
//!
//! Boa keeps script frames on the heap (`Vm::frames`, a `Vec`), so plain
//! JS-to-JS recursion costs no native stack and the frame limit can sit
//! where V8's does in practice (about 10,000 frames). What does recurse on
//! the native stack is a host call that re-enters the VM: a getter, a
//! setter, a `map` or `forEach` callback, a Proxy trap. Each such level
//! costs a few KiB (about 1.2 KiB in release builds, 6.5 KiB in debug), so
//! a frame count alone cannot keep a deep getter chain off the guard page.
//! The vendored Boa (third_party/boa_engine/PATCHES.md, "stack") therefore
//! also checks the native stack pointer at every call against a floor set
//! here from the thread's real stack bounds; past either limit the call
//! throws a catchable `RangeError`, as V8 does.

/// Script frames (JS-to-JS calls) before a call throws `RangeError`.
/// Chrome manages roughly 10,000 to 12,000 small frames; Boa's default
/// is 512, which React's commit phase on large apps outgrows.
pub const RECURSION_LIMIT: usize = 10_000;

/// Values on Boa's operand stack (heap memory) before a call throws. A
/// small frame takes 10 to 20; this leaves room for 10,000 frames of up
/// to about 100 values each.
pub const VALUE_STACK_LIMIT: usize = 1 << 20;

/// The native stack kept below the floor, for the work done between two
/// calls (a native builtin's own frames, the error being built): a
/// quarter of the thread's stack, at least 256 KiB, at most 1 MiB.
fn reserve(size: usize) -> usize {
    (size / 4).clamp(256 * 1024, 1024 * 1024)
}

/// The lowest address a call may start at on the calling thread, or
/// `None` where the stack bounds are not known (then only the frame
/// limits apply).
pub fn native_stack_floor() -> Option<usize> {
    let (low, size) = current_thread_stack()?;
    let reserve = reserve(size);
    if size <= reserve {
        return None;
    }
    Some(low + reserve)
}

/// The calling thread's stack as (lowest address, size in bytes).
#[cfg(target_os = "linux")]
fn current_thread_stack() -> Option<(usize, usize)> {
    // SAFETY: `attr` is initialised by `pthread_getattr_np` before it is
    // read and destroyed exactly once after.
    unsafe {
        let mut attr: libc::pthread_attr_t = std::mem::zeroed();
        if libc::pthread_getattr_np(libc::pthread_self(), &mut attr) != 0 {
            return None;
        }
        let mut addr: *mut libc::c_void = std::ptr::null_mut();
        let mut size: libc::size_t = 0;
        let ok = libc::pthread_attr_getstack(&attr, &mut addr, &mut size) == 0;
        libc::pthread_attr_destroy(&mut attr);
        (ok && !addr.is_null() && size > 0).then_some((addr as usize, size))
    }
}

/// The calling thread's stack as (lowest address, size in bytes).
#[cfg(target_vendor = "apple")]
fn current_thread_stack() -> Option<(usize, usize)> {
    // SAFETY: both calls only read the calling thread's own descriptor.
    let (top, size) = unsafe {
        let this = libc::pthread_self();
        (
            libc::pthread_get_stackaddr_np(this) as usize,
            libc::pthread_get_stacksize_np(this),
        )
    };
    // `pthread_get_stackaddr_np` is the top: stacks grow down.
    (top > size && size > 0).then(|| (top - size, size))
}

#[cfg(not(any(target_os = "linux", target_vendor = "apple")))]
fn current_thread_stack() -> Option<(usize, usize)> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floor_is_inside_this_thread_and_below_us() {
        let floor = native_stack_floor().expect("stack bounds on this platform");
        let marker = 0u8;
        let here = std::ptr::addr_of!(marker) as usize;
        assert!(
            floor < here,
            "the floor {floor:#x} is below this frame {here:#x}"
        );
        let (low, size) = current_thread_stack().unwrap();
        assert!(
            here > low && here < low + size,
            "the bounds hold this frame"
        );
        assert_eq!(floor - low, reserve(size));
    }
}
