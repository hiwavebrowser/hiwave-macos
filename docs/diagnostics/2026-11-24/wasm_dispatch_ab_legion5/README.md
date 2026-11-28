# wasmi loop vs tail-call dispatch, JetStream 2.2's five wasm tests (WASM W1 phase 5)

legion5 (Linux x86-64), `--profile parity` (opt-level 3, no LTO), the bare-runtime harness
`crates/rustkit-js/tests/jetstream_wasm.rs`, three runs per arm (`run.sh`). "loop" is wasmi with
`portable-dispatch` (what #1453 ships); "tail" is wasmi's default tail-call dispatch.
Times are what each test reports (ms), medians of 3.

| test | loop run | tail run | loop / tail |
|---|---:|---:|---:|
| HashSet | 8,204 | 4,024 | 2.04 |
| tsf | 33,285 | 25,009 | 1.33 |
| quicksort | 199 | 87 | 2.29 |
| gcc-loops | 70,136 | 25,969 | 2.70 |
| richards | 84,953 | 84,575 | 1.00 |

Compile times are equal in both arms (23-52 ms). `wasm_tests::a_long_wasm_loop_does_not_grow_the_native_stack`
passes in both arms at opt-level 3. In the dev profile (dependencies at opt-level 2) tail-call
dispatch overflowed a 16 MiB thread on a 100,000-iteration loop (#1453), so its stack behaviour
depends on the optimizer, not on wasmi.
