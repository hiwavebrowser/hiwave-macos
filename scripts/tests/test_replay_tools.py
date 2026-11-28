#!/usr/bin/env python3
"""Run tools/replay/test_replay.py from the script-guard loop.

The HAR replay server landed with its own suite under tools/replay/, but
`for t in scripts/tests/test_*.py` never reached it. A regression in the
generate_204 shim or base64 Content-Encoding strip would pass CI until a
realsite replay failed by hand. This wrapper makes the portable suite part
of the batch gate's script guards.

ChromeReplayTests is left out on purpose: when google-chrome-stable is
present the class is not skipped, and the synthetic HAR trace has disagreed
with the pin on CI (missing inline#2 execute). That is environment-dependent;
the portable pins still run.
"""
from __future__ import annotations

import sys
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
SUITE = REPO / "tools" / "replay" / "test_replay.py"
REPLAY_DIR = SUITE.parent


def test_the_replay_suite_exists():
    assert SUITE.is_file(), f"missing {SUITE}"


def test_the_replay_suite_passes():
    sys.path.insert(0, str(REPLAY_DIR))
    import test_replay  # noqa: WPS433 — load the suite module by path

    loader = unittest.defaultTestLoader
    suite = unittest.TestSuite()
    for name in dir(test_replay):
        obj = getattr(test_replay, name)
        if not (
            isinstance(obj, type)
            and issubclass(obj, unittest.TestCase)
            and obj is not unittest.TestCase
        ):
            continue
        if name == "ChromeReplayTests":
            continue
        suite.addTests(loader.loadTestsFromTestCase(obj))
    assert suite.countTestCases() > 0, "portable replay suite loaded no tests"
    result = unittest.TextTestRunner(verbosity=1, stream=sys.stderr).run(suite)
    assert result.wasSuccessful(), (
        f"tools/replay/test_replay.py portable suite failed "
        f"(run={result.testsRun} failures={len(result.failures)} errors={len(result.errors)})"
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
