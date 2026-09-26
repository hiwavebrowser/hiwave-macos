"""Alternating A/B board: each 5-site chunk runs every arm before the next chunk.
usage: ab_board.py <run-prefix> <label=bin> [<label=bin> ...]   (log: scratch/ab-<prefix>.log)"""
import subprocess, sys
from pathlib import Path

ROOT = Path("/Users/petecopeland/Repos/.worktrees/trench-realsite")
CHUNKS = ["google youtube facebook instagram wikipedia", "amazon reddit x linkedin yahoo",
          "bing chatgpt microsoft apple netflix", "github ebay nytimes cnn weather"]
prefix, arms = sys.argv[1], [a.split("=", 1) for a in sys.argv[2:]]
log = open(ROOT / f"scratch/ab-{prefix}.log", "a")
for chunk in CHUNKS:
    for label, binary in arms:
        args = [a for s in chunk.split() for a in ("--site", s)]
        print(f"== {label} chunk: {chunk}", file=log, flush=True)
        subprocess.run(["python3", "scripts/realsite_board.py", *args, "--capture-bin", binary,
                        "--out", f"trench/realsite/runs/{prefix}-{label}"], cwd=ROOT, stdout=log, stderr=log)
for label, _ in arms:
    subprocess.run(["python3", "scripts/realsite_board.py", "--summarize", "--out",
                    f"trench/realsite/runs/{prefix}-{label}"], cwd=ROOT, stdout=log, stderr=log)
print("ALLDONE", file=log, flush=True)
