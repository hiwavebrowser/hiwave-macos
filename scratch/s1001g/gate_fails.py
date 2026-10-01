"""Every geometry-gate failure of one case in one ratchet_local arm, in document order.
usage: gate_fails.py <label> <case>"""
import json, sys

label, case = sys.argv[1:3]
j = json.load(open(f"/tmp/rl-{label}/gate-a.json"))
for k in ("cases", "results"):
    if isinstance(j, dict) and k in j:
        j = j[k]
cs = j if isinstance(j, dict) else {c["case_id"]: c for c in j}
c = cs[case]
print(case, "failures", c["geometry_failures"])
for f in c.get("failures") or c.get("fails") or []:
    sel = " > ".join(f["selector"].split(" > ")[-2:])
    print(f"{f['path']:14} {sel[:70]:70} {f.get('axis', ''):6} chrome {f.get('expected')}  rustkit {f.get('actual')}")
