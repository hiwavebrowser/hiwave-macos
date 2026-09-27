#!/usr/bin/env python3
"""Run CI's Gate A, Gate B and ratchet over parity_test.py's local captures.

usage: ratchet_local.py <repo> <label>
Re-homes parity-baseline/captures/<case>/{frame.ppm,layout.json} into
/tmp/rl-<label>/<case>/<WxH>/iter-1/capture/ (the layout CI's gates read).
"""
import shutil, subprocess, sys
from pathlib import Path

repo, label = Path(sys.argv[1]), sys.argv[2]
root = Path(f"/tmp/rl-{label}")
shutil.rmtree(root, ignore_errors=True)
for case in sorted((repo / "parity-baseline/captures").iterdir()):
    frame = case / "frame.ppm"
    if not frame.exists():
        continue
    with open(frame, "rb") as fh:
        fh.readline()
        w, h = fh.readline().split()[:2]
    dst = root / case.name / f"{w.decode()}x{h.decode()}" / "iter-1" / "capture"
    dst.mkdir(parents=True)
    shutil.copy(frame, dst)
    if (case / "layout.json").exists():
        shutil.copy(case / "layout.json", dst)
run = lambda *a: subprocess.run([sys.executable, *a], cwd=repo, capture_output=True, text=True)
a = run("scripts/layout_oracle_gate.py", "--layout-root", str(root), "--json", str(root / "gate-a.json"))
b = run("scripts/paint_oracle_gate.py", "--capture-root", str(root), "--json", str(root / "gate-b.json"))
r = run("scripts/ratchet_gate.py", "--gate-a", str(root / "gate-a.json"), "--gate-b", str(root / "gate-b.json"))
print(r.stdout[-4000:], r.stderr[-1000:])
print("ratchet exit", r.returncode)
