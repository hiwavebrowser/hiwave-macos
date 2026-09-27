#!/usr/bin/env python3
"""Run parity-capture on an html file with RK_FLEXDBG=1 and print the trace.

usage: dbg.py <repo> <html> [w h]
"""
import os, subprocess, sys

repo, html = sys.argv[1], sys.argv[2]
w, h = (sys.argv[3], sys.argv[4]) if len(sys.argv) > 4 else ("1280", "120")
env = dict(os.environ, RK_FLEXDBG="1")
p = subprocess.run(
    [f"{repo}/target/release/parity-capture", "--html-file", html, "--width", w,
     "--height", h, "--dump-frame", "/tmp/dbg.ppm", "--dump-layout", "/tmp/dbg-layout.json"],
    env=env, capture_output=True, text=True,
)
for line in (p.stderr + p.stdout).splitlines():
    if "FLEXDBG" in line:
        print(line)
print("exit", p.returncode, (p.stderr.splitlines() or [""])[-1][:200])
