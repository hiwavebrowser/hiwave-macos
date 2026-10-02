#!/usr/bin/env python3
"""Counterbalanced A/B of cascade_bench (the 2026-09-30 A/B standard).

    ab2.py <binA> <binB> [pairs] [NAME=VALUE ...]

NAME=VALUE sets an engine flag for BOTH binaries (for example
RUSTKIT_TREE_REUSE=1, to time a branch as it will run once that is default).

Each pair loads every pinned page once with each binary. The order goes
AB BA BA AB ..., so over an even number of pairs each binary runs first as
often as second. Prints every pair with the 1-minute load and the number of
layout builds each load logged, then per site the per-pair B/A, their median,
and how many pairs read below 1. A pair whose two loads logged different
build counts is dropped for that site. Use at least 10 pairs (5 AB + 5 BA);
a read above load ~6 does not count. A last line per site splits the same
pairs by layout build (first, second, ...): per-pair B/A median and the two
medians, so a change can be placed in the build it acts on.
"""
import ast
import os
import re
import statistics as st
import subprocess
import sys

BENCH = os.path.join(os.path.dirname(os.path.abspath(__file__)), "cascade_bench.py")
SITES = ("cnn", "github", "wikipedia")
bins = sys.argv[1:3]
pairs = int(sys.argv[3]) if len(sys.argv) > 3 else 10
for kv in sys.argv[4:]:
    name, _, value = kv.partition("=")
    os.environ[name] = value
    print(f"== env for both binaries: {name}={value}", flush=True)
data = []
for i in range(pairs):
    order = (0, 1) if i % 4 in (0, 3) else (1, 0)
    row, builds, per = {}, {}, {}
    for b in order:
        load = os.getloadavg()[0]
        out = subprocess.run(
            [sys.executable, BENCH, "--capture", bins[b], "--runs", "1"],
            capture_output=True, text=True,
        )
        for line in (out.stdout + out.stderr).splitlines():
            m = re.match(r"\| (\w+) \| \d+ \| (\d+) \| (\d+) \|", line)
            if m and m[1] in SITES:
                row.setdefault(m[1], {})[b] = int(m[2])
                builds.setdefault(m[1], {})[b] = int(m[3])
            m = re.match(r"\s+(\w+)\s+run 1: .*per-build (\[[^\]]*\])", line)
            if m and m[1] in SITES:
                per.setdefault(m[1], {})[b] = ast.literal_eval(m[2])
        print(f"== pair {i + 1} order {''.join('AB'[x] for x in order)} {'AB'[b]} "
              f"{os.path.basename(bins[b])} load={load:.1f} "
              + " ".join(f"{s}={row.get(s, {}).get(b)}/{builds.get(s, {}).get(b)}b"
                         for s in SITES), flush=True)
    # A load that logged a different number of layout builds did different
    # work (a late sheet or image batch); its pair says nothing about speed.
    for s in SITES:
        if len(set(builds.get(s, {}).values())) > 1:
            print(f"   pair {i + 1} {s}: build counts differ, pair dropped", flush=True)
            row.pop(s, None)
    for s in SITES:
        if s in row:
            row[s]["per"] = per.get(s, {})
    data.append(row)
for s in SITES:
    good = [r[s] for r in data
            if 0 in r.get(s, {}) and 1 in r[s] and r[s][0] > 0 and r[s][1] > 0]
    if not good:
        print(s, "no complete pairs")
        continue
    ratios = [r[1] / r[0] for r in good]
    print(f"{s}: B/A over {len(good)} pairs: " + " ".join(f"{x:.2f}" for x in ratios)
          + f" | median {st.median(ratios):.3f} | below 1 in {sum(x < 1 for x in ratios)}"
          f" | A med {st.median(r[0] for r in good)} B med {st.median(r[1] for r in good)}",
          flush=True)
    split = [r["per"] for r in good if len(r.get("per", {})) == 2]
    for k in range(min((len(v) for p in split for v in p.values()), default=0)):
        fr = [p[1][k] / p[0][k] for p in split if p[0][k] > 0]
        if fr:
            print(f"   build {k + 1}: B/A median {st.median(fr):.3f} | below 1 in "
                  f"{sum(x < 1 for x in fr)} of {len(fr)} | A med "
                  f"{st.median(p[0][k] for p in split):.1f} B med "
                  f"{st.median(p[1][k] for p in split):.1f}", flush=True)
print(os.popen("uptime").read())
