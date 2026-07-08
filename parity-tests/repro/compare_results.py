import json, subprocess, pathlib
REPO = pathlib.Path("/Users/petecopeland/Repos/hiwave/hiwave-macos")
f = "parity-baseline/parity_test_results.json"
after = json.loads((REPO / f).read_text())
before = json.loads((REPO / "parity-tests/repro/before_results.json").read_text())

def cases(d):
    # results structure: list under 'results' or 'cases'
    for key in ("results", "cases", "tests"):
        if key in d:
            return d[key]
    return d

def to_map(d):
    m = {}
    c = cases(d)
    items = c.values() if isinstance(c, dict) else c
    for e in items:
        if not isinstance(e, dict):
            continue
        name = e.get("case") or e.get("name") or e.get("case_id")
        dp = e.get("diff_pct", e.get("diffPercent"))
        if name is not None and dp is not None:
            m[name] = dp
    return m

b, a = to_map(before), to_map(after)
keys = sorted(set(b) | set(a))
print(f"{'case':28} {'before':>8} {'after':>8} {'delta':>8}")
tot_b = tot_a = 0; n = 0
for k in keys:
    bv, av = b.get(k), a.get(k)
    if bv is None or av is None:
        print(f"{k:28} {str(bv):>8} {str(av):>8}  (missing)"); continue
    d = av - bv
    mark = "  <-- moved" if abs(d) > 0.01 else ""
    print(f"{k:28} {bv:8.2f} {av:8.2f} {d:+8.2f}{mark}")
    tot_b += bv; tot_a += av; n += 1
if n:
    print(f"{'AVG':28} {tot_b/n:8.2f} {tot_a/n:8.2f} {(tot_a-tot_b)/n:+8.2f}")
