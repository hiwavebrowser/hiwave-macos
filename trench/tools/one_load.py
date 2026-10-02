#!/usr/bin/env python3
"""one_load.py <binary> <url> [NAME=VALUE ...]: one live load, print exit code and the tail of stdout and stderr.

verify_sweep.py keeps only stderr; a load that fails before the engine exists
(2026-10-02: "TLS error: no usable platform root certificates" at load 28-34)
says why on stdout.
"""
import os
import subprocess
import sys

binary, url = sys.argv[1:3]
env = dict(os.environ, RUSTKIT_CASCADE_TIMING="1", RUST_LOG="warn,rustkit_engine=info", NO_COLOR="1",
           **dict(kv.split("=", 1) for kv in sys.argv[3:]))
p = subprocess.run([binary, "--url", url, "--width", "1280", "--height", "800", "--timeout-ms", "60000"],
                   env=env, capture_output=True, text=True, errors="replace", timeout=150)
print("exit", p.returncode)
print("--- stdout tail\n" + p.stdout[-1500:])
print("--- stderr tail\n" + p.stderr[-2500:])
