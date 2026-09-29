#!/usr/bin/env python3
"""Interleaved A/B pairs for the cascade trench (the proof standard for a fix).

    python3 trench/tools/cascade_ab.py --a <capture A> --b <capture B> [--pairs 4] [--sites wikipedia]

Each pair loads every site once with A, then once with B, back to back, and
prints cascade ms and B/A per site. Also prints the 1-minute load average
at the start of each pair, since absolute numbers only count on a quiet machine.
"""
import argparse
import os
import statistics
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cascade_bench import CHROME_STYLE_MS, run_once, serve  # noqa: E402


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--a", required=True)
    ap.add_argument("--b", required=True)
    ap.add_argument("--pairs", type=int, default=4)
    ap.add_argument("--sites", default=",".join(CHROME_STYLE_MS))
    ap.add_argument("--timeout-ms", type=int, default=120000)
    args = ap.parse_args()

    srv = serve()
    port = srv.server_address[1]
    sites = args.sites.split(",")
    ratios = {s: [] for s in sites}
    for i in range(args.pairs):
        print("pair %d  load %.1f" % (i + 1, os.getloadavg()[0]), flush=True)
        for site in sites:
            url = "http://127.0.0.1:%d/%s/index.html" % (port, site)
            ra = run_once(args.a, url, args.timeout_ms)
            rb = run_once(args.b, url, args.timeout_ms)
            ok = ra["exit"] == 0 and rb["exit"] == 0 and ra["cascade_ms"] > 0
            r = rb["cascade_ms"] / ra["cascade_ms"] if ok else None
            if r is not None:
                ratios[site].append(r)
            print("  %-10s A %7.1f ms (%d builds)  B %7.1f ms (%d builds)  B/A %s" % (
                site, ra["cascade_ms"], ra["builds"], rb["cascade_ms"], rb["builds"],
                "%.2f" % r if r is not None else "FAIL"), flush=True)
    srv.shutdown()
    print("\nmedian B/A: " + ", ".join(
        "%s %.2f" % (s, statistics.median(v)) if v else "%s -" % s for s, v in ratios.items()))


if __name__ == "__main__":
    main()
