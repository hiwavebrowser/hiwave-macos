#!/usr/bin/env python3
"""PAINT-0 follow-up: per-section y/h table, flat vs metrics vs Chrome.

Finds which section(s) hold the metrics-model height overshoot that pushes
the bottom half of css-selectors ~10px below Chrome.
"""
import glob
import json
import sys

sys.path.insert(0, "scripts")
from probe_text_metrics_coupling import flatten

fl = flatten(json.load(open("parity-tests/probe/flat/css-selectors.layout.json"))["root"])
me = flatten(json.load(open("parity-tests/probe/metrics/css-selectors.layout.json"))["root"])
flm = {b["path"]: b for b in fl}
mem = {b["path"]: b for b in me}

rects = glob.glob("baselines/chrome-148/*/css-selectors/layout-rects.json")
ch = json.load(open(rects[0]))["elements"]
chsec = {}
for e in ch:
    s = e["selector"]
    if s.startswith("body > div.section") and s.count(">") == 1:
        chsec[s.split("(")[1].rstrip(")")] = e["rect"]

hdr = "section |  flat y/h       | metrics y/h     | chrome y/h      | dh(me-fl) | me_h-ch_h | fl_h-ch_h"
print(hdr)
for n in range(8):
    p = (0, n)
    f, m = flm.get(p), mem.get(p)
    c = chsec.get(str(n + 1))
    if not (f and m and c):
        print(n + 1, "missing", bool(f), bool(m), bool(c))
        continue
    print("%7d | %7.1f %7.1f | %7.1f %7.1f | %7.1f %7.1f | %+9.2f | %+9.2f | %+9.2f"
          % (n + 1, f["y"], f["h"], m["y"], m["h"], c["y"], c["height"],
             m["h"] - f["h"], m["h"] - c["height"], f["h"] - c["height"]))
