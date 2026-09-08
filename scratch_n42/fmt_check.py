#!/usr/bin/env python3
"""Compute my own hunk line ranges from `git diff -U0` and run fmt_mine.py on them."""
import re
import subprocess
import sys

files = sys.argv[1:] or ['crates/rustkit-layout/src/flex.rs', 'crates/rustkit-layout/src/grid.rs']
args = []
for f in files:
    d = subprocess.run(['git', 'diff', '-U0', '--', f], capture_output=True, text=True).stdout
    ranges = []
    for m in re.finditer(r'^@@ -\d+(?:,\d+)? \+(\d+)(?:,(\d+))? @@', d, re.M):
        start = int(m.group(1))
        count = int(m.group(2)) if m.group(2) is not None else 1
        if count > 0:
            ranges.append('%d-%d' % (start, start + count - 1))
    if ranges:
        args.append(f + ':' + ','.join(ranges))
print('ranges:', args)
r = subprocess.run(['python3', 'scratch_n42/fmt_mine.py'] + args, capture_output=True, text=True)
print(r.stdout[-3000:])
print(r.stderr[-1000:])
print('rc', r.returncode)
