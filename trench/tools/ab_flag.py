#!/usr/bin/env python3
"""Counterbalanced A/B of one engine flag on one binary (the 2026-09-30 A/B standard).

    ab_flag.py <parity-capture> <NAME=VALUE> [pairs]

A is the binary as it is, B is the same binary with NAME=VALUE in its
environment. Otherwise this is ab2.py: each pair loads every pinned page once
each way, in the order AB BA BA AB ..., and prints every load with the
1-minute load average and its layout-build count, then per site the per-pair
B/A, their median and how many pairs read below 1. A pair whose two loads
logged different build counts is dropped for that site. Use at least 10 pairs
(5 AB + 5 BA); a read above load ~6 does not count.
"""
import os
import re
import statistics as st
import subprocess
import sys

BENCH = os.path.join(os.path.dirname(os.path.abspath(__file__)), "cascade_bench.py")
SITES = ("cnn", "github", "wikipedia")
binary, flag = sys.argv[1:3]
pairs = int(sys.argv[3]) if len(sys.argv) > 3 else 10
data = []
for i in range(pairs):
    order = (0, 1) if i % 4 in (0, 3) else (1, 0)
    row, builds = {}, {}
    for b in order:
        load = os.getloadavg()[0]
        out = subprocess.run(
            [sys.executable, BENCH, "--capture", binary, "--runs", "1"]
            + (["--env", flag] if b else []),
            capture_output=True, text=True,
        )
        for line in (out.stdout + out.stderr).splitlines():
            m = re.match(r"\| (\w+) \| \d+ \| (\d+) \| (\d+) \|", line)
            if m and m[1] in SITES:
                row.setdefault(m[1], {})[b] = int(m[2])
                builds.setdefault(m[1], {})[b] = int(m[3])
        print(f"== pair {i + 1} order {''.join('AB'[x] for x in order)} {'AB'[b]} "
              f"{flag if b else 'flag unset'} load={load:.1f} "
              + " ".join(f"{s}={row.get(s, {}).get(b)}/{builds.get(s, {}).get(b)}b"
                         for s in SITES), flush=True)
    for s in SITES:
        if len(set(builds.get(s, {}).values())) > 1:
            print(f"   pair {i + 1} {s}: build counts differ, pair dropped", flush=True)
            row.pop(s, None)
    data.append(row)
for s in SITES:
    good = [r[s] for r in data if len(r.get(s, {})) == 2 and all(v > 0 for v in r[s].values())]
    if not good:
        print(s, "no complete pairs")
        continue
    ratios = [r[1] / r[0] for r in good]
    print(f"{s}: B/A over {len(good)} pairs: " + " ".join(f"{x:.2f}" for x in ratios)
          + f" | median {st.median(ratios):.3f} | below 1 in {sum(x < 1 for x in ratios)}"
          f" | A med {st.median(r[0] for r in good)} B med {st.median(r[1] for r in good)}",
          flush=True)
print(os.popen("uptime").read())
