#!/usr/bin/env python3
"""trace.py HTML [W H] [PATTERN] — run parity-capture with RUST_LOG=rustkit_layout=trace and print matching log lines."""
import os
import re
import subprocess
import sys

html = sys.argv[1]
w = sys.argv[2] if len(sys.argv) > 2 else "800"
h = sys.argv[3] if len(sys.argv) > 3 else "700"
pat = sys.argv[4] if len(sys.argv) > 4 else r"auto-repeat|Grid layout: container|Collapsed"
env = dict(os.environ, RUST_LOG="rustkit_layout=trace")
r = subprocess.run(
    ["./target/release/parity-capture", "--html-file", html, "--width", w, "--height", h,
     "--dump-layout", "/tmp/n41-trace.layout.json"],
    capture_output=True, text=True, timeout=120, env=env)
out = r.stdout + r.stderr
n = 0
for line in out.splitlines():
    if re.search(pat, line):
        print(line[:220])
        n += 1
        if n > 60:
            break
print("rc", r.returncode, "lines", len(out.splitlines()))
