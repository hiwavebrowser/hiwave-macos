"""Which geometry-gate failures differ between two arms' gate-a.json.
usage: gate_a_cmp.py <dev-label> <fix-label> <case>..."""
import json, sys

dev, fix = sys.argv[1:3]
A = json.load(open(f"/tmp/rl-{dev}/gate-a.json"))
B = json.load(open(f"/tmp/rl-{fix}/gate-a.json"))


def cases(j):
    if isinstance(j, dict):
        for k in ("cases", "results"):
            if k in j:
                j = j[k]
                break
    if isinstance(j, dict):
        return j
    return {c.get("case") or c.get("case_id") or c.get("name"): c for c in j}


ca, cb = cases(A), cases(B)
for name in sys.argv[3:]:
    a, b = ca[name], cb[name]
    print(name, "keys:", list(a.keys())[:12])
    fa = a.get("failures") or a.get("fails") or []
    fb = b.get("failures") or b.get("fails") or []
    key = lambda f: json.dumps({k: f[k] for k in f if k in ("selector", "path", "id", "node", "key")}, sort_keys=True)
    da, db = {key(f): f for f in fa}, {key(f): f for f in fb}
    print(" dev fails", len(fa), "fix fails", len(fb))
    for k in db:
        if k not in da:
            print("  NEW in fix:", json.dumps(db[k])[:420])
    for k in da:
        if k not in db:
            print("  GONE in fix:", json.dumps(da[k])[:420])
