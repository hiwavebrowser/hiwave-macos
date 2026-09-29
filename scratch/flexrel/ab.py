#!/usr/bin/env python3
"""usage: ab.py <oracle-run> <site>... — capture base (gridrem) and flexrel back to back per site,
then report frame-vs-frame % and each one's diff against the oracle run's Chrome capture.
Run from the hub worktree root."""
import json, os, subprocess, sys
from pathlib import Path
import numpy as np
from PIL import Image

sys.path.insert(0, "scripts")
import realsite_board as rb

ARMS = {
    "base": os.environ.get("AB_BASE", "scratch/bin/parity-capture-dev-8567760"),
    "flex": os.environ.get("AB_HEAD", "scratch/bin/parity-capture-flexrel-62b05b0"),
}
for flag, arm in (("--base", "base"), ("--head", "flex")):  # --base/--head <binary> override the env
    if flag in sys.argv:
        i = sys.argv.index(flag)
        ARMS[arm] = sys.argv[i + 1]
        del sys.argv[i:i + 2]
if "--reverse" in sys.argv:  # capture head first; labels stay base/flex
    sys.argv.remove("--reverse")
    ARMS = dict(reversed(list(ARMS.items())))
urls = {s["id"]: s["url"] for s in json.load(open("websuite/realsite-top20.json"))["sites"]}
oracle = Path(sys.argv[1])
for site in sys.argv[2:]:
    frames = {}
    for arm, binary in ARMS.items():
        out = Path(f"scratch/flexrel/ab-{arm}-{site}.ppm")
        subprocess.run([binary, "--url", urls[site], "--width", "1280", "--height", "800",
                        "--timeout-ms", "90000", "--dump-frame", str(out)], capture_output=True, text=True)
        frames[arm] = out
    try:
        x, y = (np.asarray(Image.open(frames[a]).convert("RGB"), dtype=np.int16) for a in ARMS)
    except Exception as e:
        print(f"{site:10} capture failed: {e}", flush=True)
        continue
    moved = 100 * (np.abs(x - y).max(axis=2) > 8).mean()
    chrome = next((p for p in (oracle / site / "chrome-a.png", oracle / site / "chrome-b.png") if p.exists()), None)
    d = {a: (rb.pixel_diff(str(chrome), str(f), None, dict(os.environ)).get("diffPercent") if chrome else float("nan"))
         for a, f in frames.items()}
    print(f"{site:10} frames differ {moved:5.2f}%   vs chrome: base {d['base']:.2f}%  flex {d['flex']:.2f}%", flush=True)
