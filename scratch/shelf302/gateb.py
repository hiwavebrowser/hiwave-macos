#!/usr/bin/env python3
"""Score a parity-capture shelf frame with Gate B (paint) the way CI does.

usage: gateb.py <repo> <label> [frame.ppm layout.json]
Copies the capture into /tmp/gb-<label>/shelf/1280x120/iter-1/capture and runs
paint_oracle_gate.py --case shelf from <repo>.
"""
import shutil, subprocess, sys
from pathlib import Path

repo = Path(sys.argv[1])
label = sys.argv[2]
if len(sys.argv) > 4:
    frame, layout = Path(sys.argv[3]), Path(sys.argv[4])
else:
    frame = repo / "parity-baseline/captures/shelf/frame.ppm"
    layout = repo / "parity-baseline/captures/shelf/layout.json"
dst = Path(f"/tmp/gb-{label}/shelf/1280x120/iter-1/capture")
dst.mkdir(parents=True, exist_ok=True)
shutil.copy(frame, dst / "frame.ppm")
shutil.copy(layout, dst / "layout.json")
subprocess.run(
    [sys.executable, str(repo / "scripts/paint_oracle_gate.py"),
     "--capture-root", f"/tmp/gb-{label}", "--case", "shelf", "--verbose",
     "--json", f"/tmp/gb-{label}/gate-b.json"],
    cwd=repo,
)
