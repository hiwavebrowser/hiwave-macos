#!/usr/bin/env python3
"""Pixel receipt for an engine env flag on the pinned cascade snapshots.

    python3 trench/tools/flag_frames.py --capture <parity-capture> --env NAME=VALUE [--out DIR]

Per site, captures the frame three times: flag off, flag on, flag off again.
The off/off pair is the control: images and fonts still resolve to their
origins, so two unflagged loads can differ on their own. Prints the share of
differing bytes for off/on and off/off.
"""
import argparse
import os
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cascade_bench import CHROME_STYLE_MS, serve  # noqa: E402


def capture(binary, url, frame, extra_env):
    env = dict(os.environ, NO_COLOR="1", RUST_LOG="warn")
    env.update(extra_env)
    p = subprocess.run([binary, "--url", url, "--width", "1280", "--height", "800",
                        "--timeout-ms", "120000", "--dump-frame", frame],
                       env=env, capture_output=True, text=True, errors="replace")
    return p.returncode


def diff_share(a, b):
    x, y = open(a, "rb").read(), open(b, "rb").read()
    if len(x) != len(y):
        return 1.0
    return sum(1 for i, j in zip(x, y) if i != j) / max(len(x), 1)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--capture", required=True)
    ap.add_argument("--env", action="append", required=True, metavar="NAME=VALUE")
    ap.add_argument("--sites", default=",".join(CHROME_STYLE_MS))
    ap.add_argument("--out", default="/tmp/flag-frames")
    args = ap.parse_args()
    flag = dict(kv.split("=", 1) for kv in args.env)
    os.makedirs(args.out, exist_ok=True)
    srv = serve()
    port = srv.server_address[1]
    for site in args.sites.split(","):
        url = "http://127.0.0.1:%d/%s/index.html" % (port, site)
        frames = {}
        for name, env in (("off1", {}), ("on", flag), ("off2", {})):
            frames[name] = os.path.join(args.out, "%s-%s.ppm" % (site, name))
            rc = capture(args.capture, url, frames[name], env)
            if rc != 0:
                print("%-10s %s: capture exit %d" % (site, name, rc))
        print("%-10s off/on %.4f%%   off/off (control) %.4f%%" % (
            site, 100 * diff_share(frames["off1"], frames["on"]),
            100 * diff_share(frames["off1"], frames["off2"])), flush=True)
    srv.shutdown()


if __name__ == "__main__":
    main()
