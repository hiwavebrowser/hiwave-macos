#!/usr/bin/env python3
"""JIT-plan measurement: each workload on RustKit's interpreter and on V8 at each tier.

Writes one JSON line per (workload, engine) with every iteration's ms (the timer wraps one
runIteration() call; see jsbench.rs / jsbench.mjs). Steady state = median of iterations 2..N. A later line for the same (workload, engine) replaces
an earlier one (report.py keeps the last).

  python3 docs/diagnostics/2026-11-15/run_tiers.py --suites ~/zg-measure/stress-suites/JetStream \
      --rustkit target/release/examples/jsbench --node node --out out.jsonl [--only richards,...]
"""
import argparse, json, os, statistics, subprocess, sys, time

WORKLOADS = {
    "richards": ["Octane/richards.js"],
    "delta-blue": ["Octane/deltablue.js"],
    "raytrace": ["Octane/raytrace.js"],
    "navier-stokes": ["Octane/navier-stokes.js"],
    "crypto": ["Octane/crypto.js"],
    "splay": ["Octane/splay.js"],
    "earley-boyer": ["Octane/earley-boyer.js"],
    "Babylon": ["ARES-6/Babylon/index.js", "ARES-6/Babylon/benchmark.js"],
    "Air": "ARES-6/Air",
    "Basic": "ARES-6/Basic",
    "cdjs": "cdjs",
    "hash-map": ["simple/hash-map.js"],
    "UniPoker": ["RexBench/UniPoker/poker.js", "RexBench/UniPoker/expected.js", "RexBench/UniPoker/benchmark.js"],
    "ai-astar": ["SeaMonster/ai-astar.js"],
    "stanford-crypto-sha256": ["SeaMonster/sjlc.js", "SeaMonster/stanford-crypto-sha256.js"],
    "OfflineAssembler": "RexBench/OfflineAssembler",
}
DATA = {"Babylon": ["./ARES-6/Babylon/air-blob.js", "./ARES-6/Babylon/basic-blob.js",
                    "./ARES-6/Babylon/inspector-blob.js", "./ARES-6/Babylon/babylon-blob.js"]}
# Workloads given as a directory take their file list, in order, from JetStreamDriver.js.
V8_TIERS = {
    "v8-interpreter": ["--jitless"],
    "v8-sparkplug-lazy": ["--no-maglev", "--no-turbofan"],
    "v8-sparkplug-always": ["--always-sparkplug", "--no-maglev", "--no-turbofan"],
    "v8-maglev": ["--no-turbofan"],
    "v8-full": [],
}

def driver_files(suites, name):
    import re
    s = open(os.path.join(suites, "JetStreamDriver.js")).read()
    i = s.find('name: "%s"' % name); j = s.find("\n    }", i)
    blk = s[i:j].split("preload")[0]
    return [f[2:] for f in re.findall(r'"(\./[^"]+\.js)"', blk)]

def run(cmd, cwd, timeout):
    t = time.time()
    try:
        p = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        return {"error": f"timeout after {timeout}s"}
    if p.returncode != 0:
        lines = (p.stderr or p.stdout).strip().splitlines()
        # The harness panics with the script's error on the line after "panicked at".
        msg = next((lines[i + 1] for i, l in enumerate(lines[:-1]) if "panicked at" in l), lines[-1] if lines else f"exit {p.returncode}")
        return {"error": msg[:400]}
    line = p.stdout.strip().splitlines()[-1]
    r = json.loads(line)
    r["wall_s"] = round(time.time() - t, 1)
    return r

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--suites", required=True)
    ap.add_argument("--rustkit", required=True)
    ap.add_argument("--node", required=True)
    ap.add_argument("--harness", default=os.path.join(os.path.dirname(os.path.abspath(__file__)), "jsbench.mjs"))
    ap.add_argument("--out", required=True)
    ap.add_argument("--only", default="")
    ap.add_argument("--engines", default="", help="comma list of rustkit and V8 tier names; default all")
    ap.add_argument("--rk-iterations", type=int, default=6)
    ap.add_argument("--v8-iterations", type=int, default=15)
    a = ap.parse_args()
    only = set(filter(None, a.only.split(",")))
    out = open(a.out, "a")
    for name, files in WORKLOADS.items():
        if only and name not in only:
            continue
        if isinstance(files, str):
            files = driver_files(a.suites, name)
        data = []
        for d in DATA.get(name, []):
            data += ["--data", d]
        runs = {"rustkit": [a.rustkit, "--iterations", str(a.rk_iterations)] + data + files}
        for tier, flags in V8_TIERS.items():
            runs[tier] = [a.node] + flags + [a.harness, "--iterations", str(a.v8_iterations)] + data + files
        wanted = set(filter(None, a.engines.split(",")))
        for engine, cmd in runs.items():
            if wanted and engine not in wanted:
                continue
            r = run(cmd, a.suites, 1800)
            r.update({"workload": name, "engine": engine})
            it = r.get("iteration_ms")
            if it:
                r["first_ms"] = it[0]
                r["steady_ms"] = round(statistics.median(it[1:] or it), 2)
            out.write(json.dumps(r) + "\n"); out.flush()
            print(name, engine, r.get("steady_ms", r.get("error")), file=sys.stderr, flush=True)

if __name__ == "__main__":
    main()
