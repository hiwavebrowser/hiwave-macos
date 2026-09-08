#!/usr/bin/env python3
"""trace_peek.py NEEDLE [AFTER] — strip ANSI from scratch_n42/trace.log and print AFTER lines following each line containing NEEDLE."""
import re
import sys

needle = sys.argv[1]
after = int(sys.argv[2]) if len(sys.argv) > 2 else 4
ansi = re.compile(r'\x1b\[[0-9;]*m')
lines = [ansi.sub('', l) for l in open('scratch_n42/trace.log').read().splitlines()]
shown = 0
for i, l in enumerate(lines):
    if needle in l:
        for j in range(i, min(i + 1 + after, len(lines))):
            print(lines[j][35:330])
        print('...')
        shown += 1
        if shown >= 6:
            break
