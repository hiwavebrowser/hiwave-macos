#!/usr/bin/env python3
"""Sample one pinned-snapshot load with macOS `sample` (where does cascade time go?).

    python3 trench/tools/cascade_profile.py --capture <parity-capture> --site wikipedia --out prof.txt [--secs 8]

Prints the heaviest frames under build_layout_from_document from the call
tree (self-time is in the `sample` report's "Sort by top of stack" section).
"""
import argparse
import os
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cascade_bench import serve  # noqa: E402


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--capture", required=True)
    ap.add_argument("--site", default="wikipedia")
    ap.add_argument("--secs", type=int, default=8)
    ap.add_argument("--out", required=True)
    args = ap.parse_args()
    srv = serve()
    url = "http://127.0.0.1:%d/%s/index.html" % (srv.server_address[1], args.site)
    p = subprocess.Popen([args.capture, "--url", url, "--timeout-ms", "120000"],
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    time.sleep(0.3)
    subprocess.run(["sample", str(p.pid), str(args.secs), "1", "-mayDie", "-file", args.out],
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    p.wait()
    srv.shutdown()
    lines = open(args.out, errors="replace").read().split("Sort by top of stack")
    print(lines[1][:4000] if len(lines) > 1 else "no top-of-stack section")


if __name__ == "__main__":
    main()
