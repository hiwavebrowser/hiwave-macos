#!/bin/bash
# Loop (portable) vs tail-call dispatch, parity profile, the five JetStream wasm tests, 3 runs each.
cd /home/petec/repos/hiwave-macos-w1
T=/home/petec/repos/.talos-target/mw1
OUT=/home/petec/repos/.talos-wt/far_mac/dispatch_ab
mkdir -p $OUT
run() {  # $1 = label
  for i in 1 2 3; do
    /home/petec/repos/talos_test_env.sh env CARGO_TARGET_DIR=$T timeout 3600 cargo test --offline -q --profile parity -p rustkit-js --test jetstream_wasm -- --ignored --nocapture 2>&1 | grep JETSTREAM-WASM > $OUT/$1_$i.txt
  done
  /home/petec/repos/talos_test_env.sh env CARGO_TARGET_DIR=$T timeout 1800 cargo test --offline -q --profile parity -p rustkit-js -- wasm_tests::a_long_wasm_loop > $OUT/$1_stack.txt 2>&1
}
run loop
cp crates/rustkit-wasm/Cargo.toml $OUT/Cargo.toml.orig
sed -i 's/features = \["std", "validate", "portable-dispatch"\]/features = ["std", "validate"]/' crates/rustkit-wasm/Cargo.toml
grep -n '^wasmi = ' crates/rustkit-wasm/Cargo.toml > $OUT/tail_feature_line.txt
run tail
cp $OUT/Cargo.toml.orig crates/rustkit-wasm/Cargo.toml
git status --short > $OUT/final_status.txt
echo done > $OUT/DONE
