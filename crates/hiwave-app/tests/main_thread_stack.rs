//! The hiwave binary asks the linker for a 16 MiB main-thread stack
//! (build.rs), because page script runs on the AppKit main thread.

#[cfg(target_os = "macos")]
#[test]
fn hiwave_binary_has_a_16_mib_main_thread_stack() {
    let out = std::process::Command::new("otool")
        .args(["-l", env!("CARGO_BIN_EXE_hiwave")])
        .output()
        .expect("run otool");
    assert!(out.status.success(), "otool failed");
    let text = String::from_utf8_lossy(&out.stdout);
    // LC_MAIN's `stacksize` line; 0 means the 8 MiB OS default.
    let stacksize = text
        .lines()
        .skip_while(|l| !l.contains("cmd LC_MAIN"))
        .find_map(|l| l.trim().strip_prefix("stacksize "))
        .expect("LC_MAIN stacksize in otool -l output");
    assert_eq!(stacksize.trim(), "16777216");
}
