#!/usr/bin/env python3
"""Run parity-capture on an arbitrary HTML file (same invocation parity_test.py uses)."""
import subprocess
import sys

html = sys.argv[1]
w = sys.argv[2] if len(sys.argv) > 2 else '800'
h = sys.argv[3] if len(sys.argv) > 3 else '400'
stem = html.rsplit('.', 1)[0]
cmd = [
    './target/release/parity-capture',
    '--html-file', html,
    '--width', w, '--height', h,
    '--dump-frame', stem + '.ppm',
    '--dump-layout', stem + '.layout.json',
]
r = subprocess.run(cmd, capture_output=True, text=True, timeout=120)
print(r.stdout)
print(r.stderr, file=sys.stderr)
sys.exit(r.returncode)
