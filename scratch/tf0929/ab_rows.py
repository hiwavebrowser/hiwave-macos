"""Per-site side-by-side of two run dirs: points, rustkit status/error, access, looks-right.
usage: ab_rows.py <runA> <runB>"""
import json, os, sys

a, b = sys.argv[1:3]


def row(run, site):
    p = f"{run}/{site}.json"
    if not os.path.exists(p):
        return None
    d = json.load(open(p))
    rk = d.get("rustkit") or {}
    acc = d.get("access") or {}
    lr = d.get("looks_right") or {}
    return (d.get("points"), rk.get("status"), (rk.get("error") or "")[:70],
            rk.get("elapsed_ms"), acc.get("status") if isinstance(acc, dict) else acc,
            acc.get("blocked") if isinstance(acc, dict) else None,
            lr.get("diff_pct") if isinstance(lr, dict) else lr)


sites = sorted({f[:-5] for f in os.listdir(a) if f.endswith(".json") and f != "summary.json"}
               | {f[:-5] for f in os.listdir(b) if f.endswith(".json") and f != "summary.json"})
for s in sites:
    print(f"{s:12}", "A", row(a, s))
    print(f"{'':12}", "B", row(b, s))
