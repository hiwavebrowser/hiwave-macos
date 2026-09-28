#!/usr/bin/env python3
"""usage: log_phases.py <stderr-log>... — the timestamp gap before each log line, largest first."""
import re, sys
from datetime import datetime

ansi = re.compile(r"\x1b\[[0-9;]*m")
for path in sys.argv[1:]:
    rows, prev = [], None
    for line in open(path, errors="replace"):
        line = ansi.sub("", line).rstrip()
        m = re.match(r"(\d{4}-\d\d-\d\dT[\d:.]+)Z", line)
        if not m:
            continue
        t = datetime.fromisoformat(m.group(1)[:26])
        if prev is not None:
            rows.append(((t - prev[0]).total_seconds(), prev[1][28:170], line[28:170]))
        prev = (t, line)
    print(f"== {path}")
    for gap, before, after in sorted(rows, reverse=True)[:8]:
        print(f"{gap:7.2f}s  after: {before}\n          next:  {after}")
