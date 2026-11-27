//! Converting one typed array into another of a different element type (the
//! TypedArray constructor, %TypedArray%.prototype.set and slice): integer
//! targets wrap modulo 2^n (ToInt8, ToUint8, ... ToUint32), Uint8Clamped rounds
//! half to even (ToUint8Clamp). Chrome for Testing 148.0.7778.216:
//! docs/diagnostics/2026-11-24/typed_array_conversion_cft148.json, from
//! tools/parity_oracle/typed_array_conversion_probe.js (this test runs the same file).
//!
//! Boa used Rust `as` casts, which saturate: `new Uint8Array(int8Array)` made
//! every negative element 0. emscripten's in-memory file system writes files
//! with exactly that (`new Uint8Array(HEAP8.subarray(...))`), so JetStream
//! 2.2's tsf-wasm read back a corrupted file and aborted.

use rustkit_js::JsRuntime;

#[test]
fn typed_array_conversions_wrap_and_clamp_as_in_chrome() {
    let mut rt = JsRuntime::new().unwrap();
    let got = rt
        .evaluate_script(include_str!("../../../tools/parity_oracle/typed_array_conversion_probe.js"))
        .unwrap();
    let expected = [
        "u8<-i8:200,255,5,128,127",
        "u16<-i8:65480,65535,5,65408,127",
        "u32<-i8:4294967240,4294967295,5,4294967168,127",
        "i8<-u8:-56,-1,-128,5",
        "u8<-f64:44,255,0,0,0,0,2,3,254",
        "i8<-f64:44,-1,0,0,0,0,2,3,-2",
        "i32<-f64:300,-1,1410065408,0,0,0,2,3,254",
        "u8c<-f64:255,0,255,0,255,0,2,4,254",
        "i16<-i32:-1,4464,-4464",
        "u16<-i32:65535,4464,61072",
    ]
    .join("\n");
    assert_eq!(format!("{got:?}"), format!("String({expected:?})"));
}
