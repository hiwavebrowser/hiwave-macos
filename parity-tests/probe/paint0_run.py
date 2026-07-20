#!/usr/bin/env python3
"""PAINT-0 P0a/P0c runner: render the dense-text fixture with the seating
probe on and save the PAINT0 log lines.

Usage: paint0_run.py <out-log> [--html FILE] [--width W] [--height H]

The parity-capture binary renders through the real rustkit-layout ->
rustkit-renderer path; RUSTKIT_PAINT_PROBE=1 makes both crates emit PAINT0
lines on stderr (layout y_cmd chain, paint baseline/glyph_y, atlas hashes).
"""
import argparse
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
CAPTURE_BIN = os.path.join(REPO, "target", "release", "parity-capture")

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("out_log")
    ap.add_argument("--html", default=os.path.join(HERE, "dense-text.html"))
    ap.add_argument("--width", default="900")
    ap.add_argument("--height", default="1600")
    args = ap.parse_args()

    env = dict(os.environ)
    env["RUSTKIT_PAINT_PROBE"] = "1"

    ppm = os.path.splitext(args.out_log)[0] + ".ppm"
    r = subprocess.run(
        [CAPTURE_BIN, "--html-file", args.html,
         "--width", args.width, "--height", args.height,
         "--dump-frame", ppm],
        capture_output=True, text=True, env=env,
    )
    lines = [l for l in r.stderr.splitlines() if l.startswith("PAINT0 ")]
    with open(args.out_log, "w") as f:
        f.write("\n".join(lines) + "\n")
    print(f"exit={r.returncode} paint0_lines={len(lines)} -> {args.out_log}")
    if r.returncode != 0:
        print("STDOUT tail:", r.stdout[-400:])
        print("STDERR tail:", "\n".join(r.stderr.splitlines()[-10:]))
        sys.exit(1)

if __name__ == "__main__":
    main()
