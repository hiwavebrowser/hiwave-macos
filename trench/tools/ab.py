#!/usr/bin/env python3
"""Interleaved A/B of cascade_bench.

    ab.py <binA> <binB> [pairs]                  two builds
    ab.py <bin> <bin> [pairs] --b-env NAME=VALUE  one build, an engine flag off (A) vs on (B)
"""
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
argv = sys.argv[1:]
b_env = []
while "--b-env" in argv:
    i = argv.index("--b-env")
    b_env += ["--env", argv[i + 1]]
    del argv[i:i + 2]
a, b = argv[0], argv[1]
n = int(argv[2]) if len(argv) > 2 else 3
for i in range(1, n + 1):
    for side, v, extra in (("A", a, []), ("B", b, b_env)):
        print(f"== pair {i} {side} {os.path.basename(v)} {' '.join(extra[1::2])}", flush=True)
        out = subprocess.run(
            [sys.executable, os.path.join(HERE, "cascade_bench.py"), "--capture", v, "--runs", "1"] + extra,
            capture_output=True, text=True,
        )
        for line in (out.stdout + out.stderr).splitlines():
            if any(s in line for s in ("cnn", "github", "wikipedia")):
                print(line, flush=True)
print(os.popen("uptime").read())
