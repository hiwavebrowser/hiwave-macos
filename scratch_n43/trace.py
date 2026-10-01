#!/usr/bin/env python3
"""trace.py HTML W H [needle...] — run parity-capture with RUST_LOG=rustkit_layout=trace, save stderr to scratch_n42/trace.log, print lines matching any needle."""
import os
import subprocess
import sys

html, w, h = sys.argv[1], sys.argv[2], sys.argv[3]
needles = sys.argv[4:] or ['apply_positions']
env = dict(os.environ, RUST_LOG='rustkit_layout=trace')
cmd = [
    './target/release/parity-capture', '--html-file', html, '--width', w, '--height', h,
    '--dump-frame', 'scratch_n42/trace.ppm', '--dump-layout', 'scratch_n42/trace.layout.json',
]
r = subprocess.run(cmd, capture_output=True, text=True, timeout=300, env=env)
out = r.stdout + r.stderr
open('scratch_n42/trace.log', 'w').write(out)
n = 0
for line in out.splitlines():
    if any(k in line for k in needles):
        print(line[:400])
        n += 1
        if n > 80:
            break
print('rc', r.returncode, 'stderr lines', len(r.stderr.splitlines()))
