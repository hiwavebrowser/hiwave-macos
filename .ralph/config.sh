#!/bin/bash
# Ralph Configuration for HiWave

# Quality gates
export VERIFY_BEFORE_COMPLETE=true
export AUTOFIX_PRETTIER=false  # We use rustfmt, not prettier

# Iteration limits
export MAX_ITERATIONS=50
export MAX_ATTEMPTS_PER_FEATURE=3

# Testing
export RUN_TESTS_AFTER_CHANGES=true
export ALLOW_REGRESSIONS=false

# Rust-specific settings
export RUST_BACKTRACE=1
export CARGO_INCREMENTAL=1
