#!/usr/bin/env python3
"""Per-site points and metrics for two or more board runs side by side.

usage: cmp_runs.py <run-dir> <run-dir> ...
"""
import json, sys
from pathlib import Path

SITES = [s["id"] if isinstance(s, dict) else s for s in json.load(open("websuite/realsite-top20.json"))["sites"]] \
    if isinstance(json.load(open("websuite/realsite-top20.json")), dict) else None


def load(run):
    out = {}
    for f in sorted(Path(run).glob("*.json")):
        if f.name == "summary.json":
            continue
        d = json.load(open(f))
        out[f.stem] = d
    return out


runs = [load(r) for r in sys.argv[1:]]
sites = sorted(set().union(*[r.keys() for r in runs]))


def cell(d):
    if not d:
        return "-"
    pts = d.get("points", "?")
    rd = d.get("readable", {}) or {}
    lr = d.get("looks_right", {}) or {}
    rv = rd.get("coverage") if isinstance(rd, dict) else None
    lv = lr.get("diff_pct") if isinstance(lr, dict) else None
    f = lambda v: f"{v*100 if v is not None and v <= 1 else v:.1f}" if isinstance(v, (int, float)) else "-"
    return f"{pts} r{f(rv)} l{f(lv)}"


print("site".ljust(11) + "".join(Path(r).name[-22:].ljust(26) for r in sys.argv[1:]))
for s in sites:
    print(s.ljust(11) + "".join(cell(r.get(s)).ljust(26) for r in runs))
