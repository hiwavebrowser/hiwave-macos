#!/usr/bin/env python3
"""timing_lines.py <log> [...]: the Cascade timing / restyle / tree reuse / share lines of engine logs, trimmed."""
import re
import sys

ANSI = re.compile(r"\x1b\[[0-9;]*m")
KEYS = ("Cascade timing", "Incremental restyle", "Tree reuse", "Style share", "Match share", "Walk timing")
for path in sys.argv[1:]:
    print("== " + path.rsplit("/", 1)[-1])
    for line in open(path, errors="replace"):
        line = ANSI.sub("", line)
        if any(k in line for k in KEYS):
            print("  " + line.split("rustkit_engine: ")[-1].strip()[:260])
