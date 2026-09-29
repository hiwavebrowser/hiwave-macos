#!/usr/bin/env python3
"""Per-build cascade_ms of one load per site, interleaved across binaries.

    build_split.py <bin> [<bin> ...] [--rounds N] [--sites cnn,github,wikipedia]

A whole-load B/A drifts with machine load. The split between a load's
builds (build 1 vs builds 2..n, same process, same second) drifts much
less, so a change that only touches later builds shows there even at
load 15-20.
"""
import argparse
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cascade_bench import run_once, serve  # noqa: E402


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("bins", nargs="+")
    ap.add_argument("--rounds", type=int, default=2)
    ap.add_argument("--sites", default="cnn,github,wikipedia")
    args = ap.parse_args()
    srv = serve()
    port = srv.server_address[1]
    for rnd in range(1, args.rounds + 1):
        for site in args.sites.split(","):
            url = "http://127.0.0.1:%d/%s/index.html" % (port, site)
            for b in args.bins:
                ms = [round(c) for c in run_once(b, url, 120000)["per_build_ms"]]
                rest = sum(ms[1:])
                print("round %d %-9s %-16s builds %s  first %d  rest %d  rest/first %.2f"
                      % (rnd, site, os.path.basename(b), ms, ms[0] if ms else 0, rest,
                         rest / ms[0] if ms and ms[0] else 0), flush=True)
    srv.shutdown()
    print(os.popen("uptime").read().strip())


if __name__ == "__main__":
    main()
