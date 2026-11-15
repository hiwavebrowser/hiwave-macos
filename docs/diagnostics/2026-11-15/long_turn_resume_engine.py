#!/usr/bin/env python3
"""PACE1, the engine arms: run the tests that load long_turn_resume.html N times each and collect the
PACE1-RESULT line every run prints (long_turn_resume_engine.json).

  app-content-thread  rustkit-engine live_pacing_tests: the app's default loop (PageHalf::run)
  pace1-content       parity-capture --live-ms, default pacing (the content thread's wait)
  pace1-window        parity-capture --live-ms --window-thread-pacing

Usage, from the repository root:
  python3 docs/diagnostics/2026-11-15/long_turn_resume_engine.py [RUNS, default 3] > long_turn_resume_engine.json
Each arm is a wall-clock measurement of the machine it runs on: the record names the host.
"""
import json
import platform
import subprocess
import sys

ARMS = [
    (["cargo", "test", "-q", "-p", "rustkit-engine", "--lib", "--features", "headless", "live_pacing_tests"]),
    (["cargo", "test", "-q", "-p", "parity-capture", "--", "long_frame", "window_thread_pacing_waits"]),
]


def main():
    runs = int(sys.argv[1]) if len(sys.argv) > 1 else 3
    results = []
    for argv in ARMS:
        for _ in range(runs):
            p = subprocess.run(argv + ["--", "--nocapture"] if "--" not in argv else argv + ["--nocapture"],
                               capture_output=True, text=True, encoding="utf-8", errors="replace")
            if p.returncode != 0:
                sys.exit("%s failed (exit %d):\n%s" % (" ".join(argv), p.returncode, p.stdout[-2000:] + p.stderr[-2000:]))
            for line in (p.stdout + p.stderr).splitlines():
                if line.startswith("PACE1-RESULT "):
                    results.append(json.loads(line[len("PACE1-RESULT "):]))
    head = subprocess.run(["git", "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip()
    print(json.dumps({"head": head, "host": {"system": platform.system(), "machine": platform.machine()},
                      "runs_per_arm": runs, "results": results}, indent=1))


if __name__ == "__main__":
    main()
