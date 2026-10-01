#!/usr/bin/env python3
"""Pool ab2.py logs: per site, the median of per-pair B/A.

    ab2_summary.py <log> [<log> ...] [--max-load 6]

A pair counts only if both of its loads started at or below --max-load and
(when the log records them) logged the same number of layout builds. Prints
the AB/BA split, so a pooled read can be checked against the A/B standard
(at least 5 AB and 5 BA pairs).
"""
import re
import statistics as st
import sys

argv = sys.argv[1:]
max_load = 6.0
if "--max-load" in argv:
    i = argv.index("--max-load")
    max_load = float(argv[i + 1])
    del argv[i:i + 2]
SITES = ("cnn", "github", "wikipedia")
LINE = re.compile(r"== pair (\d+) order (\w\w) (\w) \S+ load=([\d.]+) (.*)")
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
      f"{sum(p['order'] == 'AB' for p in quiet)} AB + {sum(p['order'] == 'BA' for p in quiet)} BA, "
      f"load {min(min(p['load']) for p in quiet):.1f}-{max(max(p['load']) for p in quiet):.1f}")
for s in SITES:
    good = [p["ms"][s] for p in quiet
            if len(p["ms"].get(s, {})) == 2 and len(set(p["builds"][s].values())) == 1]
    ratios = [g["B"] / g["A"] for g in good]
    print(f"{s}: {len(good)} pairs | median B/A {st.median(ratios):.3f} | below 1 in "
          f"{sum(r < 1 for r in ratios)} | range {min(ratios):.2f}-{max(ratios):.2f} | "
          f"A median {st.median(g['A'] for g in good)} ms, B median {st.median(g['B'] for g in good)} ms")
