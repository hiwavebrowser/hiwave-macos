#!/usr/bin/env python3
"""Capture all 26 gating cases with the release parity-capture, into <outdir>/<case>/layout.json.

NOT A RECEIPT. On a non-macOS seat these captures carry the platform confound
(fontconfig substitution, SwiftShader, a different Chromium milestone); night 44
measured it at 8.7% of the corpus and up to 88% on individual small cases. Use
`scripts/seat_control_report.py` before attributing any delta taken from these
captures to RustKit.

Diagnostic only, no mutation-checked guards — do not wire into CI.
See trench/forensics/2026-09-07-n45-per-pr-condition-board.md.
"""
import json, os, subprocess, sys, pathlib

REPO = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "scripts"))
import layout_oracle_gate as g

outroot = pathlib.Path(sys.argv[1])
reg = g.load_case_registry()
cases = {k: v for k, v in reg.items() if v.get("scope") != "holdout"}

env = dict(os.environ)
env["VK_ICD_FILENAMES"] = "/opt/pw-browsers/chromium-1194/chrome-linux/vk_swiftshader_icd.json"

ok = fail = 0
for cid, c in sorted(cases.items()):
    d = outroot / cid
    d.mkdir(parents=True, exist_ok=True)
    cmd = [
        str(REPO / "target/release/parity-capture"),
        "--html-file", str(REPO / c["html"]),
        "--width", str(c["width"]), "--height", str(c["height"]),
        "--dump-frame", str(d / "frame.png"),
        "--dump-layout", str(d / "layout.json"),
    ]
    r = subprocess.run(cmd, capture_output=True, text=True, env=env, cwd=REPO, timeout=300)
    last = (r.stdout.strip().splitlines() or [""])[-1]
    try:
        status = json.loads(last).get("status")
    except Exception:
        status = f"BADOUT rc={r.returncode}"
    if status == "ok":
        ok += 1
    else:
        fail += 1
        print(f"  CAPTURE FAIL {cid}: {status}", flush=True)
print(f"captured ok={ok} fail={fail} -> {outroot}")
