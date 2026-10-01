#!/usr/bin/env python3
"""find.py FILE SUBSTR [SUBSTR...] — print line numbers + text of lines containing any substring."""
import sys
path = sys.argv[1]
needles = sys.argv[2:]
for i, line in enumerate(open(path, encoding='utf-8', errors='replace'), 1):
    if any(n in line for n in needles):
        print(i, line.rstrip()[:140])
