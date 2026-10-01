"""History of one site's LOADS evidence across quiet board runs.
usage: hist.py <site>..."""
import json, os, sys

root = "trench/realsite/runs"
for r in sorted(os.listdir(root)):
    if "quiet" not in r:
        continue
    for s in sys.argv[1:]:
        p = os.path.join(root, r, s + ".json")
        if not os.path.exists(p):
            continue
        z = json.load(open(p))
        rk = z.get("rustkit") or {}
        print(r, s, z.get("points"), rk.get("non_background_fraction"), rk.get("elapsed_ms"), (z.get("loads") or {}).get("why"))
