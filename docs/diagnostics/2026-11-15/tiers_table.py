#!/usr/bin/env python3
"""Summarise jit_plan_tiers.jsonl: steady-state ms per engine and the ratios the plan uses.
The last line for a (workload, engine) wins. Geometric means over the workloads that ran on every
engine."""
import json, math, sys
rows = {}
for l in open(sys.argv[1] if len(sys.argv) > 1 else "jit_plan_tiers.jsonl"):
    r = json.loads(l)
    rows[(r["workload"], r["engine"])] = r
eng = ["rustkit", "v8-interpreter", "v8-sparkplug-lazy", "v8-sparkplug-always", "v8-maglev", "v8-full"]
work = list(dict.fromkeys(w for w, _ in rows))
def st(w, e):
    r = rows.get((w, e)); return r.get("steady_ms") if r else None
print("| workload | RustKit | V8 interp | V8 +Sparkplug (lazy) | V8 +Sparkplug (always) | V8 +Maglev | V8 all tiers | RustKit / V8 interp | V8 interp / +Sparkplug(always) | V8 interp / all tiers |")
print("|---|---|---|---|---|---|---|---|---|---|")
g = {"rk_i": [], "i_sp": [], "i_full": []}
for w in work:
    v = [st(w, e) for e in eng]
    def f(x): return "error" if x is None else f"{x:.1f}"
    ratio = lambda a, b: (a / b) if (a and b) else None
    a, b, c = ratio(v[0], v[1]), ratio(v[1], v[3]), ratio(v[1], v[5])
    if None not in v:
        g["rk_i"].append(a); g["i_sp"].append(b); g["i_full"].append(c)
    rf = lambda x: "-" if x is None else f"{x:.1f}x"
    err = rows.get((w, "rustkit"), {}).get("error")
    print(f"| {w} | {f(v[0]) if not err else 'fails'} | " + " | ".join(f(x) for x in v[1:]) + f" | {rf(a)} | {rf(b)} | {rf(c)} |")
gm = lambda xs: math.exp(sum(map(math.log, xs)) / len(xs))
print(f"\nGeometric mean over the {len(g['rk_i'])} workloads that ran everywhere: RustKit / V8 interpreter {gm(g['rk_i']):.1f}x; "
      f"V8 interpreter / +Sparkplug(always) {gm(g['i_sp']):.2f}x; V8 interpreter / all tiers {gm(g['i_full']):.1f}x")
