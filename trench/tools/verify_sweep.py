#!/usr/bin/env python3
"""Load every real-site board URL once with an engine flag in verify mode and
print what the engine's verify lines said.

    verify_sweep.py <parity-capture> <sites.json> <outdir> [NAME=VALUE ...]

Default env is RUSTKIT_TREE_REUSE=verify. Per site: exit code, the cascade ms
of each layout build, and every "Tree reuse" / "Incremental restyle" /
"Style share" line. The
engine log of each load is kept in <outdir>/<site>.log. Ends with the number
of sites that verified a tree and the total of their mismatches, and the
totals of the "Style share" lines (hits, mismatches, parent mismatches). The sites are
the board's pinned list (websuite/realsite-top20.json), loaded live.
"""
import json
import os
import re
import subprocess
import sys

binary, sites_file, outdir = sys.argv[1:4]
flags = dict(kv.split("=", 1) for kv in sys.argv[4:]) or {"RUSTKIT_TREE_REUSE": "verify"}
os.makedirs(outdir, exist_ok=True)
ANSI = re.compile(r"\x1b\[[0-9;]*m")
cfg = json.load(open(sites_file))
width, height = cfg["viewport"]["width"], cfg["viewport"]["height"]
verified = boxes = mismatches = 0
share = {"hits": 0, "mismatches": 0, "parent_mismatches": 0}
share_sites = 0
for site in cfg["sites"]:
    env = dict(os.environ, RUSTKIT_CASCADE_TIMING="1",
               RUST_LOG="warn,rustkit_engine=info", NO_COLOR="1", **flags)
    try:
        p = subprocess.run([binary, "--url", site["url"], "--width", str(width),
                            "--height", str(height), "--timeout-ms", "60000"],
                           env=env, capture_output=True, text=True, errors="replace",
                           timeout=120)
        code, log = p.returncode, ANSI.sub("", p.stderr)
    except subprocess.TimeoutExpired as e:
        code, log = "killed", ANSI.sub("", (e.stderr or b"").decode(errors="replace"))
    open(os.path.join(outdir, site["id"] + ".log"), "w").write(log)
    lines = log.splitlines()
    builds = [round(float(m.group(1))) for m in
              (re.search(r"cascade_ms=([\d.]+)", l) for l in lines if "Cascade timing" in l) if m]
    said = [l.split("rustkit_engine: ")[-1][:130] for l in lines
            if "Tree reuse" in l or "Incremental restyle" in l or "Style share" in l]
    share_sites += any(s.startswith("Style share") for s in said)
    for s in said:
        m = re.match(r"Tree reuse verify boxes=(\d+) mismatches=(\d+)", s)
        if m:
            verified += 1
            boxes += int(m.group(1))
            mismatches += int(m.group(2))
        if s.startswith("Style share"):
            for name in share:
                m = re.search(r"\b%s=(\d+)" % name, s)
                share[name] += int(m.group(1)) if m else 0
    print("%-10s exit %s builds %s %s" % (site["id"], code, builds, said), flush=True)
print("tree verifies: %d, boxes %d, mismatches %d" % (verified, boxes, mismatches))
print("style share: %d sites, " % share_sites + ", ".join("%s %d" % kv for kv in share.items()))
