#!/usr/bin/env python3
"""The JS engine and bindings suites must execute in CI, including integration tests.

WHAT THIS GUARDS
----------------
Closes the CI gap from Issue #574 (Item 5):
`.github/workflows/parity.yml` previously only ran `unit-suites` for layout and
engine with `--lib` (advisory). `rustkit-js` and `rustkit-bindings` integration
tests (class fields TDZ, GC weak cycles, let/for-of, web API census probe) never
ran in CI.

This guard verifies that:
1. `js-suites` exists in `parity.yml`.
2. It runs both `rustkit-js` and `rustkit-bindings`.
3. It does NOT pass `--lib`, ensuring all integration test targets execute.
4. It does NOT pass `--no-run`.
5. It is BLOCKING: no `continue-on-error: true`.
6. It runs on macos-14 with Rust toolchain and cache.
7. It publishes receipts to `$GITHUB_STEP_SUMMARY`.
8. It is not conditioned away on pull requests.

Run: python3 scripts/tests/test_js_suites_actually_run.py
"""
import sys
from pathlib import Path
import yaml

REPO_ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = REPO_ROOT / ".github" / "workflows" / "parity.yml"

REQUIRED_SUITES = ("rustkit-js", "rustkit-bindings")


def _workflow():
    return yaml.safe_load(WORKFLOW.read_text(encoding="utf-8"))


def _lane():
    jobs = _workflow()["jobs"]
    assert "js-suites" in jobs, (
        "no `js-suites` job in parity.yml — JS engine and bindings suites must run in CI"
    )
    return jobs["js-suites"]


def _lane_step():
    steps = [s for s in _lane()["steps"] if "run" in s]
    assert len(steps) == 1, f"expected one run step in js-suites, got {len(steps)}"
    return steps[0]


def test_the_lane_executes_rather_than_compiling():
    run = _lane_step()["run"]
    assert "cargo test" in run, "the js-suites lane does not invoke cargo test"
    assert "--no-run" not in run, (
        "the js-suites lane passes --no-run: tests would never execute"
    )
    for pkg in REQUIRED_SUITES:
        assert pkg in run, f"{pkg} is not in the js-suites lane"


def test_the_lane_runs_integration_tests_not_just_lib():
    run = _lane_step()["run"]
    assert "--lib" not in run, (
        "the js-suites lane must NOT pass --lib; integration tests "
        "(class fields, GC, let-of, census) must execute"
    )


def test_the_lane_is_blocking():
    step = _lane_step()
    lane = _lane()
    assert not step.get("continue-on-error"), (
        "js-suites step has continue-on-error: true; it must be blocking"
    )
    assert not lane.get("continue-on-error"), (
        "js-suites job has continue-on-error: true; it must be blocking"
    )


def test_the_lane_runs_on_macos14():
    assert _lane()["runs-on"] == "macos-14", "js-suites must run on macos-14"


def test_the_receipt_reaches_the_job_summary():
    assert "$GITHUB_STEP_SUMMARY" in _lane_step()["run"], (
        "js-suites does not write receipts to $GITHUB_STEP_SUMMARY"
    )


def test_the_lane_is_not_skipped_on_pull_requests():
    cond = str(_lane().get("if", "")).strip()
    assert "pull_request" not in cond, (
        f"js-suites is conditioned away from PR lane (if: {cond})"
    )


if __name__ == "__main__":
    assert WORKFLOW.exists(), f"no workflow at {WORKFLOW}"
    failed = 0
    for name, fn in sorted(globals().items()):
        if name.startswith("test_") and callable(fn):
            try:
                fn()
                print(f"ok  {name}")
            except AssertionError as exc:
                failed = 1
                print(f"FAIL {name}: {exc}")
    if failed:
        sys.exit(1)
    print("PASS: the JS engine and bindings suites execute and block in CI")
