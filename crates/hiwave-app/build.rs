fn main() {
    // Page script runs on the AppKit main thread in the shipped app, and Boa
    // re-enters the VM on the native stack for every host callback. The
    // macOS default main-thread stack is 8 MiB; ask the linker for 16 MiB,
    // the same size rustkit_js::spawn_script_thread gives script threads.
    // Remove this once the M4 content thread runs page script on its own
    // thread with an explicit stack.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-arg-bins=-Wl,-stack_size,0x1000000");
    }
}
