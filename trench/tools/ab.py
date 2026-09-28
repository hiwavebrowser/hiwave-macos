#!/usr/bin/env python3
"""Interleaved A/B of cascade_bench: ab.py <binA> <binB> [pairs]."""
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
a, b = sys.argv[1], sys.argv[2]
n = int(sys.argv[3]) if len(sys.argv) > 3 else 3
for i in range(1, n + 1):
    for v in (a, b):
        print(f"== pair {i} {os.path.basename(v)}", flush=True)
        out = subprocess.run(
            [sys.executable, os.path.join(HERE, "cascade_bench.py"), "--capture", v, "--runs", "1"],
            capture_output=True, text=True,
        )
        for line in (out.stdout + out.stderr).splitlines():
            if any(s in line for s in ("cnn", "github", "wikipedia")):
                print(line, flush=True)
print(os.popen("uptime").read())
