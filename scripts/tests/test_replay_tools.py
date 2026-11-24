#!/usr/bin/env python3
"""Run tools/replay/test_replay.py from the script-guard loop.

The HAR replay server landed with its own suite under tools/replay/, but
`for t in scripts/tests/test_*.py` never reached it. A regression in the
generate_204 shim or base64 Content-Encoding strip would pass CI until a
realsite replay failed by hand. This wrapper makes the suite part of the
batch gate's script guards.
"""
from __future__ import annotations

import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
SUITE = REPO / "tools" / "replay" / "test_replay.py"


def test_the_replay_suite_exists():
    assert SUITE.is_file(), f"missing {SUITE}"


def test_the_replay_suite_passes():
    proc = subprocess.run(
        [sys.executable, str(SUITE)],
        cwd=str(REPO),
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0:
        sys.stderr.write(proc.stdout)
        sys.stderr.write(proc.stderr)
    assert proc.returncode == 0, (
        f"tools/replay/test_replay.py failed with exit {proc.returncode}"
    )


if __name__ == "__main__":
    failed = 0
    for name, fn in sorted(globals().items()):
        if name.startswith("test_") and callable(fn):
            try:
                fn()
                print(f"ok  {name}")
            except AssertionError as exc:
                failed = 1
                print(f"FAIL {name}: {exc}")
    raise SystemExit(failed)
