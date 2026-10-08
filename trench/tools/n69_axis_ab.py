#!/usr/bin/env python3
"""Per-AXIS A/B of two Gate A reports: which boxes got better, which got worse.

NOT A RECEIPT. On a non-macOS seat these captures carry the platform confound;
`trench/tools/n67_confound_census.py` says which axes may be attributed at all.

A case-level failure COUNT hides both halves of what the stop rule asks about: a
box that got worse under one that got better, and a magnitude-only change that
moves no count. This joins the two reports on (case, path, selector, axis) and
buckets every axis into improved / worsened / appeared / disappeared, where
`appeared` is an axis that was inside tolerance before and fails now. A
disappeared axis is reported as an improvement of unknown magnitude, never
folded into `sum|delta|`, because a passing axis has no delta in the report.

Diagnostic only, no mutation-checked guards — do not wire into CI.

  n69_axis_ab.py <gateA-before.json> <gateA-after.json>
"""
import json, sys, collections

def load(p):
    d = json.load(open(p))
    out, per_case = {}, {}
    for c in d["cases"]:
        per_case[c["case_id"]] = {
            "geometry_failures": c["geometry_failures"],
            "join_failures": c["join_failures"],
            "green": c["green"],
            "measured": c["measured"],
        }
        for f in c["failures"]:
            if f.get("kind") != "delta":
                continue
            out[(c["case_id"], f["path"], f["selector"], f["axis"])] = f
    return out, per_case

before, bc = load(sys.argv[1])
after, ac = load(sys.argv[2])

improved, worsened, appeared, disappeared = [], [], [], []
for k in set(before) | set(after):
    b, a = before.get(k), after.get(k)
    if b and a:
        db, da = abs(b["delta"]), abs(a["delta"])
        if da < db - 1e-4:
            improved.append((k, b["delta"], a["delta"]))
        elif da > db + 1e-4:
            worsened.append((k, b["delta"], a["delta"]))
    elif a:
        appeared.append((k, None, a["delta"]))
    else:
        disappeared.append((k, b["delta"], None))

sb = sum(abs(f["delta"]) for f in before.values())
sa = sum(abs(f["delta"]) for f in after.values())
gb = sum(v["geometry_failures"] for v in bc.values())
ga = sum(v["geometry_failures"] for v in ac.values())
print(f"geometry failures   {gb} -> {ga}")
print(f"geometry-green      {sum(1 for v in bc.values() if v['green'])}/{len(bc)} -> "
      f"{sum(1 for v in ac.values() if v['green'])}/{len(ac)}")
print(f"join failures       {sum(v['join_failures'] for v in bc.values())} -> "
      f"{sum(v['join_failures'] for v in ac.values())}")
print(f"sum|delta|          {sb:.2f} -> {sa:.2f}  ({sa-sb:+.2f})")
print(f"axes improved {len(improved)}   WORSENED {len(worsened)}   "
      f"appeared {len(appeared)}   disappeared {len(disappeared)}")

for name, rows in (("WORSENED", worsened), ("improved", improved),
                   ("APPEARED (was inside tolerance)", appeared),
                   ("disappeared (now inside tolerance)", disappeared)):
    if not rows:
        continue
    print(f"\n--- {name} ({len(rows)}) ---")
    rows.sort(key=lambda r: -(abs(r[2] if r[2] is not None else 0) -
                              abs(r[1] if r[1] is not None else 0)))
    for (case, path, sel, axis), b, a in rows:
        bs = "  (passing)" if b is None else f"{b:+9.3f}"
        as_ = "  (passing)" if a is None else f"{a:+9.3f}"
        print(f"  {case:<24} {sel:<44} {axis:<7} {bs} -> {as_}")

per = collections.Counter()
for (case, _, _, _), _, _ in worsened:
    per[case] += 1
if per:
    print("\nworsened axes per case:", dict(per))
