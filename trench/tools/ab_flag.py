#!/usr/bin/env python3
"""Counterbalanced A/B of one engine flag on one binary (the 2026-09-30 A/B standard).

    ab_flag.py <parity-capture> <NAME=VALUE> [pairs] [A_NAME=VALUE]

A is the binary as it is, B is the same binary with NAME=VALUE in its
environment. With the fourth argument A gets that setting instead and B runs
with nothing set (for a flag whose default has flipped: A is the old path).
Each load line ends with the per-build cascade ms of every site. Otherwise this is ab2.py: each pair loads every pinned page once
each way, in the order AB BA BA AB ..., and prints every load with the
1-minute load average and its layout-build count, then per site the per-pair
B/A, their median and how many pairs read below 1. Every complete pair
counts; the pairs with equal build counts are summarised beside it (see
ab_pairs.py). Use at least 10 pairs (5 AB + 5 BA); a read above load ~6 does
not count. A last line per site splits the equal-build pairs by layout build.
"""
import ast
import os
import re
import statistics as st
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from ab_pairs import summarize  # noqa: E402

BENCH = os.path.join(os.path.dirname(os.path.abspath(__file__)), "cascade_bench.py")
SITES = ("cnn", "github", "wikipedia")
binary, flag = sys.argv[1:3]
pairs = int(sys.argv[3]) if len(sys.argv) > 3 else 10
# What each arm adds to the environment, and what the load lines call it.
arm_env = ([sys.argv[4]], []) if len(sys.argv) > 4 else ([], [flag])
arm_name = tuple(" ".join(e) or "flag unset" for e in arm_env)
data = {s: [] for s in SITES}
for i in range(pairs):
    order = (0, 1) if i % 4 in (0, 3) else (1, 0)
    row, builds, per = {}, {}, {}
    for b in order:
        load = os.getloadavg()[0]
        out = subprocess.run(
            [sys.executable, BENCH, "--capture", binary, "--runs", "1"]
            + [x for e in arm_env[b] for x in ("--env", e)],
            capture_output=True, text=True,
        )
        for line in (out.stdout + out.stderr).splitlines():
            m = re.match(r"\| (\w+) \| \d+ \| (\d+) \| (\d+) \|", line)
            if m and m[1] in SITES:
                row.setdefault(m[1], {})[b] = int(m[2])
                builds.setdefault(m[1], {})[b] = int(m[3])
            m = re.match(r"\s+(\w+)\s+run 1: .* per-build (\[.*\])", line)
            if m and m[1] in SITES:
                per.setdefault(m[1], {})[b] = ast.literal_eval(m[2])
        print(f"== pair {i + 1} order {''.join('AB'[x] for x in order)} {'AB'[b]} "
              f"{arm_name[b]} load={load:.1f} "
              + " ".join(f"{s}={row.get(s, {}).get(b)}/{builds.get(s, {}).get(b)}b"
                         for s in SITES)
              + " | " + " ".join(str(per.get(s, {}).get(b, [])).replace(" ", "")
                                 for s in SITES), flush=True)
    for s in SITES:
        data[s].append({
            "order": "".join("AB"[x] for x in order),
            "ms": (row.get(s, {}).get(0), row.get(s, {}).get(1)),
            "builds": (builds.get(s, {}).get(0), builds.get(s, {}).get(1)),
            "per": per.get(s, {}),
        })
for s in SITES:
    if summarize(s, data[s]) is None:
        continue
    split = [p["per"] for p in data[s]
             if len(p["per"]) == 2 and len(p["per"][0]) == len(p["per"][1])]
    for k in range(min((len(p[0]) for p in split), default=0)):
        fr = [p[1][k] / p[0][k] for p in split if p[0][k] > 0]
        if fr:
            print(f"   build {k + 1}: B/A median {st.median(fr):.3f} | below 1 in "
                  f"{sum(x < 1 for x in fr)} of {len(fr)} | A med "
                  f"{st.median(p[0][k] for p in split):.1f} B med "
                  f"{st.median(p[1][k] for p in split):.1f}", flush=True)
print(os.popen("uptime").read())
