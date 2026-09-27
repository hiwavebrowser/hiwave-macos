#!/usr/bin/env python3
"""Stale-on-resize check (PLAN 2026-09-27 16:20, Pete's "elasticity").

For each source (live URL or local HTML file):
  A  = load at 1280x800, resize the live view to WxH, capture   (--resize-to)
  B1 = fresh load at WxH
  B2 = fresh load at WxH again (self-noise: live pages rotate content)
Reports pixel diff % A-vs-B1 and B1-vs-B2 (any channel > 8/255).
A-vs-B1 well above B1-vs-B2 means layout that stayed stale across the resize.

usage: resize_check.py <capture-bin> <out-dir> <WxH> <src> [<src> ...]
  src: http(s) URL, or a path to an .html file
"""
import json
import os
import subprocess
import sys

import numpy as np


def read_ppm(path):
    with open(path, "rb") as f:
        data = f.read()
    parts = []
    i = 0
    while len(parts) < 4:
        while data[i : i + 1].isspace():
            i += 1
        if data[i : i + 1] == b"#":
            while data[i : i + 1] != b"\n":
                i += 1
            continue
        j = i
        while not data[j : j + 1].isspace():
            j += 1
        parts.append(data[i:j])
        i = j
    i += 1
    w, h = int(parts[1]), int(parts[2])
    return np.frombuffer(data[i : i + w * h * 3], dtype=np.uint8).reshape(h, w, 3)


def diff_pct(a, b):
    if a.shape != b.shape:
        return 100.0
    d = np.abs(a.astype(np.int16) - b.astype(np.int16)).max(axis=2) > 8
    return 100.0 * d.mean()


def capture(bin_, src, out, size, resize_to=None):
    w, h = size
    args = [bin_, "--width", str(w), "--height", str(h), "--dump-frame", out + ".ppm",
            "--dump-display-list", out + ".dl.json", "--timeout-ms", "45000"]
    args += ["--url", src] if src.startswith(("http", "file:")) else ["--html-file", src]
    if resize_to:
        args += ["--resize-to", "%dx%d" % resize_to]
    p = subprocess.run(args, capture_output=True, text=True, timeout=60)
    try:
        res = json.loads(p.stdout.strip().splitlines()[-1])
    except Exception:
        res = {"status": "noparse", "error": p.stderr[-300:]}
    return res


def main():
    bin_, outdir, wxh = sys.argv[1:4]
    target = tuple(int(x) for x in wxh.split("x"))
    os.makedirs(outdir, exist_ok=True)
    rows = []
    for src in sys.argv[4:]:
        name = src.split("//")[-1].strip("/").replace("/", "_") if src.startswith("http") else os.path.basename(src)
        base = os.path.join(outdir, f"{name}-{wxh}")
        ra = capture(bin_, src, base + "-A", (1280, 800), target)
        rb1 = capture(bin_, src, base + "-B1", target)
        rb2 = capture(bin_, src, base + "-B2", target)
        stat = [r.get("status") for r in (ra, rb1, rb2)]
        if all(s == "ok" for s in stat):
            a, b1, b2 = (read_ppm(base + s + ".ppm") for s in ("-A", "-B1", "-B2"))
            row = dict(src=src, size=wxh, a_vs_b1=round(diff_pct(a, b1), 2),
                       b1_vs_b2=round(diff_pct(b1, b2), 2), a_shape=list(a.shape))
        else:
            row = dict(src=src, size=wxh, status=stat,
                       errors=[r.get("error") for r in (ra, rb1, rb2)])
        rows.append(row)
        print(json.dumps(row), flush=True)
    with open(os.path.join(outdir, f"summary-{wxh}.json"), "a") as f:
        for r in rows:
            f.write(json.dumps(r) + "\n")


if __name__ == "__main__":
    main()
