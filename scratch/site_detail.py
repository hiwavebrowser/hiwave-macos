#!/usr/bin/env python3
"""usage: site_detail.py <site> <run-dir> [<run-dir> ...] — the three checks and RustKit scalars per run."""
import json, sys

site = sys.argv[1]
for run in sys.argv[2:]:
    d = json.load(open(f"{run}/{site}.json"))
    for k in ("loads", "readable", "looks_right"):
        print(run.rstrip("/").split("/")[-1], k, json.dumps(d.get(k))[:300])
    print("  rustkit", {k: v for k, v in (d.get("rustkit") or {}).items() if not isinstance(v, (list, dict))})
