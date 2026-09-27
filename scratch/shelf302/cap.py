#!/usr/bin/env python3
"""parity-capture a URL or html file with frame + layout dumps.

usage: cap.py <bin> <url-or-html> <out-prefix> [w h]
"""
import subprocess, sys

bin_, src, out = sys.argv[1:4]
w, h = (sys.argv[4], sys.argv[5]) if len(sys.argv) > 5 else ("1280", "800")
flag = "--url" if src.startswith("http") else "--html-file"
p = subprocess.run(
    [bin_, flag, src, "--width", w, "--height", h, "--dump-frame", out + ".ppm",
     "--dump-layout", out + "-layout.json"],
    capture_output=True, text=True, timeout=120,
)
print("exit", p.returncode, (p.stderr.strip().splitlines() or [""])[-1][:300])
