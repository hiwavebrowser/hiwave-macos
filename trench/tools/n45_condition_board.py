#!/usr/bin/env python3
"""Per-case Gate A board across branch capture sets.

NOT A RECEIPT. The `geometry-green /26` line this prints is finish-line
CONDITION 1 of 4 (geometry within 0.5px per box). It is NOT `N/26`
finish-line-green, which additionally requires paint >= 99% within the pinned
aa_tolerance, stability across 3 iterations, and zero discrete structural
failures. Quoting this line as the campaign metric is the exact Goodhart
substitution `trench/BASELINE-parity-finish-line.md` exists to prevent.

This script measures nothing of its own: it shells out to
`scripts/layout_oracle_gate.py` and aggregates that gate's JSON. Run it from ONE
pinned checkout over every capture set, so the instrument is identical for each
row and only the engine differs.

Diagnostic only, and it carries NO mutation-checked guards — do not wire it into
CI. See trench/forensics/2026-09-07-n45-per-pr-condition-board.md.
"""
import json, subprocess, sys, pathlib

REPO = pathlib.Path(__file__).resolve().parents[2]
# Usage: n45_condition_board.py <capture-parent-dir> <label> [<label> ...]
# expects <capture-parent-dir>/cap-<label>/<case>/layout.json
SP = pathlib.Path(sys.argv[1])
labels = sys.argv[2:]
results = {}
for lab in labels:
    root = SP / f"cap-{lab}"
    if not (root / "combinators" / "layout.json").exists():
        print(f"# {lab}: NO CAPTURES", file=sys.stderr)
        continue
    out = SP / f"gateA-{lab}.json"
    subprocess.run(
        [sys.executable, "scripts/layout_oracle_gate.py",
         "--layout-root", str(root), "--json", str(out)],
        cwd=REPO, capture_output=True, text=True)
    d = json.load(open(out))
    results[lab] = {c["case_id"]: c for c in d["cases"]}

cases = sorted(results[labels[0]].keys()) if labels and labels[0] in results else \
        sorted(next(iter(results.values())).keys())

base = "develop"
print(f"{'case':<26} " + " ".join(f"{l:>9}" for l in labels))
for cid in cases:
    row = []
    for l in labels:
        c = results.get(l, {}).get(cid)
        if c is None:
            row.append("       --")
        elif not c["measured"]:
            row.append("    UNMEA")
        elif c["green"]:
            row.append("    GREEN")
        else:
            row.append(f"{c['geometry_failures']:>5}+{c['join_failures']:<3}")
    print(f"{cid:<26} " + " ".join(f"{x:>9}" for x in row))

print()
print(f"{'geometry-green /26':<26} " + " ".join(
    f"{sum(1 for c in results.get(l,{}).values() if c['measured'] and c['green']):>9}" for l in labels))
print(f"{'total geometry failures':<26} " + " ".join(
    f"{sum(c['geometry_failures'] for c in results.get(l,{}).values()):>9}" for l in labels))
print(f"{'total join failures':<26} " + " ".join(
    f"{sum(c['join_failures'] for c in results.get(l,{}).values()):>9}" for l in labels))

# Per-branch case-level transitions vs develop
if base in results:
    print("\n=== case-level transitions vs develop (condition 1 only) ===")
    for l in labels:
        if l == base:
            continue
        newly, lost, better, worse = [], [], [], []
        for cid in cases:
            b, c = results[base].get(cid), results.get(l, {}).get(cid)
            if not b or not c or not b["measured"] or not c["measured"]:
                continue
            if c["green"] and not b["green"]:
                newly.append(cid)
            elif b["green"] and not c["green"]:
                lost.append(cid)
            else:
                db = b["geometry_failures"] + b["join_failures"]
                dc = c["geometry_failures"] + c["join_failures"]
                if dc < db:
                    better.append(f"{cid} {db}->{dc}")
                elif dc > db:
                    worse.append(f"{cid} {db}->{dc}")
        print(f"\n#{l}:")
        print(f"  NEWLY GEOMETRY-GREEN: {', '.join(newly) if newly else 'none'}")
        print(f"  LOST GREEN:           {', '.join(lost) if lost else 'none'}")
        print(f"  fewer failures:       {'; '.join(better) if better else 'none'}")
        print(f"  more failures:        {'; '.join(worse) if worse else 'none'}")
