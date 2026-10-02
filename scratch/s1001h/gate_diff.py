"""Geometry-gate failures present in one ratchet_local arm and not the other, for a case.
usage: gate_diff.py <dev-label> <fix-label> <case>"""
import json, sys


def fails(label, case):
    j = json.load(open(f"/tmp/rl-{label}/gate-a.json"))
    for k in ("cases", "results"):
        if isinstance(j, dict) and k in j:
            j = j[k]
    cs = j if isinstance(j, dict) else {c["case_id"]: c for c in j}
    out = {}
    for f in cs[case].get("failures") or cs[case].get("fails") or []:
        out[(f["path"], f.get("axis", ""))] = f
    return out


dev, fix, case = sys.argv[1:4]
a, b = fails(dev, case), fails(fix, case)
for name, only, other in (("fixed (in develop only)", a, b), ("new (in fix only)", b, a)):
    keys = [k for k in only if k not in other]
    print(f"{case}: {name}: {len(keys)}")
    for k in keys:
        f = only[k]
        sel = " > ".join(f["selector"].split(" > ")[-2:])
        print(f"  {f['path']:14} {sel[:64]:64} {f.get('axis', ''):6} chrome {f.get('expected')}  rustkit {f.get('actual')}")
