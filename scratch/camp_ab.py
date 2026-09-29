#!/usr/bin/env python3
"""usage: camp_ab.py <repo> <head-binary> <base-binary> <label>
Runs parity_test.py in <repo> with each binary swapped into target/release/parity-capture
(head last, so the repo is left with the head's captures), then prints avg + per-case deltas
and a markdown <details> table. Restores the head binary at the end."""
import json, shutil, subprocess, sys
from pathlib import Path

repo, head, base, label = Path(sys.argv[1]), sys.argv[2], sys.argv[3], sys.argv[4]
tgt = repo / "target/release/parity-capture"
res = {}
for arm, binary in (("base", base), ("head", head)):
    shutil.copy(binary, tgt)
    p = subprocess.run([sys.executable, "scripts/parity_test.py"], cwd=repo, capture_output=True, text=True)
    d = json.load(open(repo / "parity-baseline/parity_test_results.json"))
    res[arm] = {"ts": d["timestamp"], "passed": d["passed"], "cases": {r["case_id"]: r["diff_pct"] for r in d["results"]}}
    shutil.copy(repo / "parity-baseline/parity_test_results.json", f"scratch/receipts/ptr-{label}-{arm}.json")
avg = lambda c: sum(c.values()) / len(c)
b, h = res["base"], res["head"]
print(f"base {b['ts']} passed {b['passed']}/{len(b['cases'])} avg {avg(b['cases']):.4f}%")
print(f"head {h['ts']} passed {h['passed']}/{len(h['cases'])} avg {avg(h['cases']):.4f}%")
changed = [c for c in h["cases"] if abs(h["cases"][c] - b["cases"].get(c, -1)) > 1e-9]
print("changed cases:", changed or "none")
rows = "\n".join(f"| {c} | {b['cases'].get(c, float('nan')):.4f} | {h['cases'][c]:.4f} |" for c in h["cases"])
Path(f"scratch/receipts/camp-{label}.md").write_text(
    f"<details><summary>Per-case diff_pct (26 cases)</summary>\n\n| case | develop | this PR |\n|---|---|---|\n{rows}\n\n</details>\n")
print(f"table -> scratch/receipts/camp-{label}.md (run from the hub root)")
