#!/usr/bin/env python3
"""Take N `sample` profiles of one pinned-site load and pool their inclusive counts.

    cascade_prof_pool.py --capture <symbolized parity-capture> --site github --runs 6 --out-prefix ../prof-x [--top 60] [--root build_layout_from_document] [--env NAME=VALUE]

One `sample -wait` attach often catches only a slice of a ~2 s load, so a
single report can hold as few as 15 samples under the root. Pooling several
loads gives shares worth ranking. Reports land at <out-prefix>-<i>.txt.
"""
import argparse
import os
import subprocess
import sys
from collections import defaultdict

HERE = os.path.dirname(os.path.abspath(__file__))


def inclusive(path, root):
    """Parse cascade_prof_sum.py output into {frame: samples} (and the root count)."""
    out = subprocess.run([sys.executable, os.path.join(HERE, "cascade_prof_sum.py"), path, root],
                         capture_output=True, text=True).stdout.splitlines()
    counts = {}
    for line in out[1:]:
        parts = line.split(None, 2)
        if len(parts) == 3 and parts[1].isdigit():
            counts[parts[2]] = int(parts[1])
    return counts


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--capture", required=True)
    ap.add_argument("--site", default="github")
    ap.add_argument("--runs", type=int, default=6)
    ap.add_argument("--secs", type=int, default=15)
    ap.add_argument("--out-prefix", required=True)
    ap.add_argument("--root", default="build_layout_from_document")
    ap.add_argument("--top", type=int, default=60)
    ap.add_argument("--env", action="append", default=[], metavar="NAME=VALUE")
    args = ap.parse_args()
    pooled = defaultdict(int)
    for i in range(1, args.runs + 1):
        out = "%s-%d.txt" % (args.out_prefix, i)
        subprocess.run([sys.executable, os.path.join(HERE, "cascade_profile.py"), "--capture", args.capture,
                        "--site", args.site, "--secs", str(args.secs), "--out", out]
                       + [x for e in args.env for x in ("--env", e)],
                       stdout=subprocess.DEVNULL)
        c = inclusive(out, args.root)
        root_n = max(c.values()) if c else 0
        print("run %d: %d samples under root" % (i, root_n), flush=True)
        for k, v in c.items():
            pooled[k] += v
    total = max(pooled.values()) if pooled else 0
    print("pooled root samples: %d" % total)
    for k, v in sorted(pooled.items(), key=lambda kv: -kv[1])[: args.top]:
        print("%6.1f%% %6d  %s" % (100.0 * v / max(total, 1), v, k))


if __name__ == "__main__":
    main()
