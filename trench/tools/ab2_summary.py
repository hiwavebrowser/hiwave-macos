#!/usr/bin/env python3
"""Pool ab2.py / ab_flag.py logs: per site, the median of per-pair B/A.

    ab2_summary.py <log> [<log> ...] [--max-load 6]

A pair counts only if both of its loads started at or below --max-load.
Every such pair with two finished loads counts; the pairs whose loads logged
equal build counts are summarised beside it, with the AB/BA split of each
line, so a pooled read can be checked against the A/B standard (at least
5 AB and 5 BA pairs). See ab_pairs.py for why unequal pairs are kept.
"""
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from ab_pairs import summarize  # noqa: E402

argv = sys.argv[1:]
max_load = 6.0
if "--max-load" in argv:
    i = argv.index("--max-load")
    max_load = float(argv[i + 1])
    del argv[i:i + 2]
SITES = ("cnn", "github", "wikipedia")
LINE = re.compile(r"== pair (\d+) order (\w\w) (\w) .*? load=([\d.]+) ([^|]*)")
pairs = []
for path in argv:
    cur = {}
    for line in open(path):
        m = LINE.match(line)
        if not m:
            continue
        key = (path, int(m[1]))
        cur.setdefault(key, {"order": m[2], "load": [], "ms": {}, "builds": {}})
        p = cur[key]
        p["load"].append(float(m[4]))
        for site, ms, builds in re.findall(r"(\w+)=(\d+)(?:/(\d+)b)?", m[5]):
            p["ms"].setdefault(site, {})[m[3]] = int(ms)
            p["builds"].setdefault(site, {})[m[3]] = builds
    pairs += cur.values()
quiet = [p for p in pairs if len(p["load"]) == 2 and max(p["load"]) <= max_load]
print(f"{len(pairs)} pairs read, {len(quiet)} with both loads at load <= {max_load}: "
      f"{sum(p['order'] == 'AB' for p in quiet)} AB + {sum(p['order'] == 'BA' for p in quiet)} BA"
      + (f", load {min(min(p['load']) for p in quiet):.1f}-{max(max(p['load']) for p in quiet):.1f}"
         if quiet else ""))
for s in SITES:
    summarize(s, [{
        "order": p["order"],
        "ms": (p["ms"].get(s, {}).get("A"), p["ms"].get(s, {}).get("B")),
        "builds": (p["builds"].get(s, {}).get("A"), p["builds"].get(s, {}).get("B")),
    } for p in quiet])
