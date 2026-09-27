#!/usr/bin/env python3
"""A/B a live URL across two parity-capture builds, N alternating rounds.

usage: ab_url.py <url> <needle> <n> <label=bin> [<label=bin> ...]
Prints, per capture, whether <needle> appears in the layout dump.
"""
import subprocess, sys
from pathlib import Path

url, needle, n = sys.argv[1], sys.argv[2], int(sys.argv[3])
arms = [a.split("=", 1) for a in sys.argv[4:]]
out = Path(__file__).parent / "ab"
out.mkdir(exist_ok=True)
for i in range(n):
    for label, bin_ in arms:
        prefix = out / f"{label}-{i}"
        p = subprocess.run(
            [bin_, "--url", url, "--width", "1280", "--height", "800",
             "--dump-frame", f"{prefix}.ppm", "--dump-layout", f"{prefix}-layout.json", "--dump-display-list", f"{prefix}-dl.json"],
            capture_output=True, text=True, timeout=120,
        )
        layout = Path(f"{prefix}-dl.json")
        text = layout.read_text() if layout.exists() else ""
        print(label, i, "exit", p.returncode, needle, needle in text, "bytes", len(text))
