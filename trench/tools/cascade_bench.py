#!/usr/bin/env python3
"""Cascade-speed trench instrument: RustKit cascade time vs Chrome style time.

    python3 trench/tools/cascade_bench.py --capture <parity-capture> [--runs 5] [--sites cnn,github]

Serves trench/cascade/snapshots/ on 127.0.0.1 and loads each pinned page with
`parity-capture --url` at 1280x800, RUSTKIT_CASCADE_TIMING=1. The engine logs
one "Cascade timing" line per layout build (parse_ms, cascade_ms). Per run:
  cascade = sum of cascade_ms over every build in the load (Chrome's
            UpdateLayoutTree self time: style recalc + layout-tree build)
  parse   = sum of parse_ms (stylesheet extraction; Chrome counts it as parse)
Prints raw runs, the median of each site, and median cascade / Chrome style.
--json writes the same numbers to a file.
"""
import argparse
import functools
import http.server
import json
import os
import re
import statistics
import subprocess
import sys
import threading

HERE = os.path.dirname(os.path.abspath(__file__))
SNAPSHOTS = os.path.join(HERE, "..", "cascade", "snapshots")
# Chrome 148 style ms (UpdateLayoutTree + RecalculateStyles self time), from
# trench/ANALYSIS-chrome-groundtruth-2026-09-26.md. Frozen: see BASELINE-cascade.md.
CHROME_STYLE_MS = {"cnn": 210.0, "github": 110.0, "wikipedia": 20.0}
PARSE_MS = re.compile(r"parse_ms=([\d.]+)")
CASCADE_MS = re.compile(r"cascade_ms=([\d.]+)")
ANSI = re.compile(r"\x1b\[[0-9;]*m")


class Quiet(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a):
        pass


def serve():
    handler = functools.partial(Quiet, directory=SNAPSHOTS)
    srv = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    return srv


def timings(stderr):
    """(parse_ms, cascade_ms) per "Cascade timing" line, in log order."""
    out = []
    for line in ANSI.sub("", stderr).splitlines():
        if "Cascade timing" in line:
            p, c = PARSE_MS.search(line), CASCADE_MS.search(line)
            if p and c:
                out.append((float(p.group(1)), float(c.group(1))))
    return out


def run_once(capture, url, timeout_ms, log_path=None):
    env = dict(os.environ, RUSTKIT_CASCADE_TIMING="1",
               RUST_LOG="warn,rustkit_engine=info", NO_COLOR="1")
    p = subprocess.run([capture, "--url", url, "--width", "1280", "--height", "800",
                        "--timeout-ms", str(timeout_ms)],
                       env=env, capture_output=True, text=True, errors="replace")
    if log_path:
        with open(log_path, "w") as f:
            f.write(p.stderr)
    builds = timings(p.stderr)
    return {
        "exit": p.returncode,
        "builds": len(builds),
        "parse_ms": round(sum(b[0] for b in builds), 1),
        "cascade_ms": round(sum(b[1] for b in builds), 1),
        "per_build_ms": [round(b[1], 1) for b in builds],
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--capture", required=True, help="release parity-capture binary")
    ap.add_argument("--runs", type=int, default=5)
    ap.add_argument("--sites", default=",".join(CHROME_STYLE_MS))
    ap.add_argument("--timeout-ms", type=int, default=120000)
    ap.add_argument("--json")
    ap.add_argument("--logs", help="directory to keep each run's stderr")
    args = ap.parse_args()

    if args.logs:
        os.makedirs(args.logs, exist_ok=True)
    srv = serve()
    port = srv.server_address[1]
    out = {}
    for site in args.sites.split(","):
        url = "http://127.0.0.1:%d/%s/index.html" % (port, site)
        runs = []
        for i in range(args.runs):
            log = os.path.join(args.logs, "%s-%d.log" % (site, i + 1)) if args.logs else None
            r = run_once(args.capture, url, args.timeout_ms, log)
            runs.append(r)
            print("  %-10s run %d: exit %d builds %d cascade %8.1f ms parse %7.1f ms  per-build %s" % (
                site, i + 1, r["exit"], r["builds"], r["cascade_ms"], r["parse_ms"], r["per_build_ms"]),
                file=sys.stderr, flush=True)
        good = [r for r in runs if r["exit"] == 0 and r["builds"] > 0]
        med = statistics.median(r["cascade_ms"] for r in good) if good else None
        out[site] = {
            "runs": runs,
            "median_cascade_ms": med,
            "median_parse_ms": statistics.median(r["parse_ms"] for r in good) if good else None,
            "chrome_style_ms": CHROME_STYLE_MS[site],
            "ratio": round(med / CHROME_STYLE_MS[site], 1) if med else None,
        }
    srv.shutdown()

    print("\n| site | Chrome style ms | RustKit cascade ms (median of %d) | builds | parse ms | ratio | raw cascade ms |" % args.runs)
    print("|---|---|---|---|---|---|---|")
    for site, s in out.items():
        builds = sorted({r["builds"] for r in s["runs"]})
        print("| %s | %.0f | %s | %s | %s | %s | %s |" % (
            site, s["chrome_style_ms"],
            "%.0f" % s["median_cascade_ms"] if s["median_cascade_ms"] is not None else "FAIL",
            "/".join(map(str, builds)),
            "%.0f" % s["median_parse_ms"] if s["median_parse_ms"] is not None else "-",
            "%.1f×" % s["ratio"] if s["ratio"] else "-",
            ", ".join("%.0f" % r["cascade_ms"] for r in s["runs"])))
    ratios = [s["ratio"] for s in out.values() if s["ratio"]]
    worst = max(ratios) if len(ratios) == len(out) else None
    print("\nworst ratio: %s" % ("%.1f×" % worst if worst else "INCOMPLETE (a site failed)"))
    if args.json:
        with open(args.json, "w") as f:
            json.dump({"sites": out, "worst_ratio": worst}, f, indent=1)
    return 0 if worst else 1


if __name__ == "__main__":
    sys.exit(main())
