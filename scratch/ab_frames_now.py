#!/usr/bin/env python3
"""usage: ab_frames_now.py <oracle-run> <url-id>=<url>... — capture #323 and blockify back to back,
then report frame-vs-frame % and each one's diff against the oracle run's Chrome capture."""
import os, subprocess, sys
from pathlib import Path
import numpy as np
from PIL import Image

sys.path.insert(0, "scripts")
import realsite_board as rb

ARMS = {
    "gta": "/Users/petecopeland/Repos/.worktrees/rs-grid-template-areas/target/release/parity-capture",
    "blk": "/Users/petecopeland/Repos/.worktrees/rs-blockify/target/release/parity-capture",
}
if sys.argv[1] == "--reverse":
    ARMS = dict(reversed(list(ARMS.items())))
    sys.argv.pop(1)
oracle = Path(sys.argv[1])
for spec in sys.argv[2:]:
    site, url = spec.split("=", 1)
    frames = {}
    for arm, binary in ARMS.items():
        out = Path(f"/tmp/abnow-{arm}-{site}.ppm")
        subprocess.run([binary, "--url", url, "--width", "1280", "--height", "800", "--timeout-ms", "90000",
                        "--dump-frame", str(out)], capture_output=True, text=True)
        frames[arm] = out
    x, y = (np.asarray(Image.open(frames[a]).convert("RGB"), dtype=np.int16) for a in ARMS)
    moved = 100 * (np.abs(x - y).max(axis=2) > 8).mean()
    chrome = next(p for p in (oracle / site / "chrome-a.png", oracle / site / "chrome-b.png") if p.exists())
    d = {a: rb.pixel_diff(str(chrome), str(f), None, dict(os.environ)).get("diffPercent") for a, f in frames.items()}
    print(f"{site:10} frames differ {moved:5.2f}%   vs chrome: gta {d['gta']:.2f}%  blk {d['blk']:.2f}%", flush=True)
