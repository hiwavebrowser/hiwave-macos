#!/usr/bin/env python3
"""usage: same_oracle.py <oracle-run> <run>... -- <site>...
LOOKS RIGHT diff of each run's RustKit frame against ONE Chrome capture (the oracle run's
chrome-a.png, else chrome-b.png), with the board's own pixel_diff. Removes oracle drift from an A/B."""
import inspect, os, sys
from pathlib import Path

sys.path.insert(0, "scripts")
import realsite_board as rb

args = sys.argv[1:]
cut = args.index("--")
runs, sites = args[:cut], args[cut + 1:]
oracle_run = Path(runs[0])
print("pixel_diff", inspect.signature(rb.pixel_diff))
for site in sites:
    chrome = next((p for p in (oracle_run / site / "chrome-a.png", oracle_run / site / "chrome-b.png") if p.exists()), None)
    if chrome is None:
        print(f"{site:10} no oracle in {oracle_run.name}")
        continue
    row = []
    for run in runs[1:]:
        frame = Path(run) / site / "rustkit.ppm"
        res = rb.pixel_diff(str(chrome), str(frame), None, dict(os.environ)) if frame.exists() else {}
        row.append(f"{Path(run).name}={res.get('diff', res.get('diff_pct', res))}")
    print(f"{site:10} vs {chrome.parent.parent.name}/{chrome.name}: " + "  ".join(row))
