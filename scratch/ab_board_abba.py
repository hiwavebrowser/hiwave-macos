"""Full-board interleaved A/B: 4 chunks of 5 sites, arm order alternating
per chunk (ABBA...), each arm into its own run dir, then --summarize both.
usage: ab_board_abba.py <prefix> <labelA=binA> <labelB=binB>"""
import json, subprocess, sys

ROOT = "/Users/petecopeland/Repos/.worktrees/trench-realsite"
prefix = sys.argv[1]
arms = [a.split("=", 1) for a in sys.argv[2:4]]
sites = [s["id"] for s in json.load(open(f"{ROOT}/websuite/realsite-top20.json"))["sites"]]
chunks = [sites[i:i + 5] for i in range(0, len(sites), 5)]


def run(label, binary, extra):
    args = ["python3", "scripts/realsite_board.py", "--capture-bin", binary,
            "--out", f"trench/realsite/runs/{prefix}-{label}"] + extra
    subprocess.run(args, cwd=ROOT)


for i, chunk in enumerate(chunks):
    order = arms if i % 2 == 0 else arms[::-1]
    for label, binary in order:
        print(f"== chunk {i} {label}: {chunk}", flush=True)
        run(label, binary, [a for s in chunk for a in ("--site", s)])
for label, binary in arms:
    run(label, binary, ["--summarize"])
print("DONE", prefix, flush=True)
