"""All geometry-gate failures on <kbd> boxes, both arms, with Chrome's expected value.
usage: kbd_cmp.py <dev-label> <fix-label> <case> [substring]"""
import json, sys

dev, fix, case = sys.argv[1:4]
sub = sys.argv[4] if len(sys.argv) > 4 else "kbd"


def fails(label):
    j = json.load(open(f"/tmp/rl-{label}/gate-a.json"))
    for k in ("cases", "results"):
        if isinstance(j, dict) and k in j:
            j = j[k]
    cs = j if isinstance(j, dict) else {c["case_id"]: c for c in j}
    c = cs[case]
    print(label, {k: c[k] for k in ("chrome_boxes", "rustkit_boxes", "compared", "text_backend") if k in c},
          "failures", c["geometry_failures"])
    return {(f["path"], f.get("axis")): f for f in c.get("failures") or c.get("fails") or []}


a, b = fails(dev), fails(fix)
for k in sorted(set(a) | set(b)):
    f = a.get(k) or b.get(k)
    if sub not in f["selector"]:
        continue
    x, y = a.get(k), b.get(k)
    print(f"{f['selector'].split(' > ')[-2][:28]:28} > {f['selector'].split(' > ')[-1]:22} {k[1]:6} chrome {f.get('expected')}"
          f"  dev {x['actual'] if x else 'ok'}  fix {y['actual'] if y else 'ok'}")
