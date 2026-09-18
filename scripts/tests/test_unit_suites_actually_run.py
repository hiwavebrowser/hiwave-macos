#!/usr/bin/env python3
"""The Rust unit suites must actually EXECUTE in CI, and say so honestly.

WHAT THIS GUARDS
----------------
Until 2026-09-18 no lane in this repository ran a Rust test. `f1-test-compile`
is `cargo test --workspace --no-run` — it compiles every test target and stops
— the swarm lanes run the parity board, and `script-guards` runs the Python
guards. Every mutation-checked Rust guard this campaign has written, which is
the whole of its correctness evidence, executed exactly once: on the seat of
the night that wrote it. By this campaign's own standard an unrun guard is
decoration, so the `unit-suites` lane (F2) runs them on every PR and nightly.

This file holds the lane to four properties, and three of them are checked by
RUNNING the lane's own shell against stubbed cargo output rather than reading
it:

1. It executes. A `--no-run` in that lane makes it F1 with a longer comment.
2. A suite that reports nothing is a FAILURE, not a pass. `cargo test` on a
   target that ran no tests can exit 0 and print no `test result:` line; a
   lane that scored that green would be the instrument lie this campaign
   exists to end — a did-not-run wearing a green check.
3. A red suite is reported RED.
4. It runs on macos-14, and F1's whole-workspace compile falsifier survives.

Property 4 is static (there is nothing to execute); 1-3 are behavioural.

Run: python3 scripts/tests/test_unit_suites_actually_run.py
"""
import os
import subprocess
import sys
import tempfile
from pathlib import Path

import yaml

REPO_ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = REPO_ROOT / ".github" / "workflows" / "parity.yml"

# The suites the campaign's working rule names as mandatory before every
# commit. Named here so that dropping one from the lane is a red guard rather
# than a quieter lane.
REQUIRED_SUITES = ("rustkit-layout", "rustkit-engine")

GREEN_LOG = (
    "running 267 tests\n"
    "test result: ok. 267 passed; 0 failed; 0 ignored; 0 measured; "
    "0 filtered out; finished in 0.31s\n"
)
RED_LOG = (
    "running 267 tests\n"
    "failures:\n    layout::tests::a_grid_row_is_sized_from_the_margin_box\n"
    "test result: FAILED. 266 passed; 1 failed; 0 ignored; 0 measured; "
    "0 filtered out; finished in 0.29s\n"
)
# Exit 0 and not one `test result:` line. This is the shape that matters: a
# harness change that filters every test away still exits 0.
SILENT_LOG = "   Compiling rustkit-layout v0.1.0\n    Finished test profile\n"


def _workflow():
    return yaml.safe_load(WORKFLOW.read_text())


def _lane():
    jobs = _workflow()["jobs"]
    assert "unit-suites" in jobs, (
        "no `unit-suites` job in parity.yml — the Rust suites are back to "
        "never running anywhere"
    )
    return jobs["unit-suites"]


def _lane_step():
    steps = [s for s in _lane()["steps"] if "run" in s]
    assert len(steps) == 1, f"expected one run step in unit-suites, got {len(steps)}"
    return steps[0]


def _execute(behaviour):
    """Run the lane's own shell with `cargo` stubbed.

    `behaviour` maps package name -> (stdout, exit code). Returns
    (exit status, job summary text).
    """
    run = _lane_step()["run"]
    cases = "\n".join(
        f'    {pkg}) printf %s "$(cat <<\'EOF\'\n{log}EOF\n)"; return {rc} ;;'
        for pkg, (log, rc) in behaviour.items()
    )
    # A bash function shadows PATH lookup for the whole script, including the
    # pipeline into tee, so the lane runs unmodified below this.
    stub = (
        "cargo() {\n"
        '  local pkg="$3"\n'
        "  case \"$pkg\" in\n"
        f"{cases}\n"
        '    *) echo "unstubbed package $pkg" ; return 1 ;;\n'
        "  esac\n"
        "}\n"
    )
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        summary = tmp / "summary.md"
        summary.touch()
        script = tmp / "step.sh"
        script.write_text(stub + run)
        proc = subprocess.run(
            ["bash", str(script)],
            capture_output=True,
            text=True,
            env={
                # Inherited, not hardcoded: the lane's shell needs tee,
                # grep and head, and they do not live in the same directory
                # on every runner image. The cargo stub above shadows a real
                # cargo on PATH regardless, because a shell function wins.
                "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
                "RUNNER_TEMP": str(tmp),
                "GITHUB_STEP_SUMMARY": str(summary),
            },
        )
        return proc.returncode, summary.read_text()


def test_the_lane_executes_rather_than_compiling():
    run = _lane_step()["run"]
    assert "cargo test" in run, "the unit-suites lane does not invoke cargo test"
    assert "--no-run" not in run, (
        "the unit-suites lane passes --no-run, which makes it a second copy of "
        "f1-test-compile: the suites would still never execute"
    )
    for pkg in REQUIRED_SUITES:
        assert pkg in run, f"{pkg} is not in the unit-suites lane"
    assert "--lib" in run, "the lane must run the lib suites"


def test_a_green_pair_passes():
    rc, summary = _execute({p: (GREEN_LOG, 0) for p in REQUIRED_SUITES})
    assert rc == 0, f"two green suites did not pass the lane (rc={rc})\n{summary}"
    # `| green |` and not `green`: the receipt's own header says
    # "green before every commit", so a bare count reads 3 for 2 suites.
    assert summary.count("| green |") == len(REQUIRED_SUITES), summary
    for pkg in REQUIRED_SUITES:
        assert pkg in summary, f"{pkg} missing from the receipt\n{summary}"


def test_a_red_suite_fails_the_lane():
    behaviour = {REQUIRED_SUITES[0]: (RED_LOG, 101)}
    behaviour.update({p: (GREEN_LOG, 0) for p in REQUIRED_SUITES[1:]})
    rc, summary = _execute(behaviour)
    assert rc != 0, f"a failing suite scored as a pass\n{summary}"
    assert "RED" in summary, summary


def test_a_suite_that_ran_nothing_is_not_a_pass():
    """The instrument-lie shape: exit 0, no tests, no `test result:` line."""
    behaviour = {REQUIRED_SUITES[0]: (SILENT_LOG, 0)}
    behaviour.update({p: (GREEN_LOG, 0) for p in REQUIRED_SUITES[1:]})
    rc, summary = _execute(behaviour)
    assert rc != 0, (
        "a suite that executed no tests and exited 0 scored green — a "
        f"did-not-run wearing a green check\n{summary}"
    )
    assert "DID NOT RUN" in summary, summary


def test_the_receipt_reaches_the_job_summary():
    """Advisory means visible. A lane whose receipt goes nowhere is ignored by
    construction, however loud its exit status."""
    rc, summary = _execute({p: (GREEN_LOG, 0) for p in REQUIRED_SUITES})
    assert summary.strip(), "the lane wrote nothing to GITHUB_STEP_SUMMARY"
    assert "$GITHUB_STEP_SUMMARY" in _lane_step()["run"]


def test_the_lane_runs_where_coretext_is():
    assert _lane()["runs-on"] == "macos-14", (
        "the unit suites must run on macos-14: two of #199's justify tests are "
        "green on CoreText and red on a Linux font stack (measured 09-17), so "
        "a ubuntu lane would red-lock on font substitution"
    )


def test_the_lane_is_not_skipped_on_pull_requests():
    cond = str(_lane().get("if", "")).strip()
    assert "pull_request" not in cond, (
        f"unit-suites is conditioned away from the PR lane (if: {cond}); "
        "pr_merge is where this campaign enforces its bars"
    )


def test_f1_still_compiles_the_whole_workspace():
    """F2 is narrower than F1 on purpose — it runs two packages, F1 compiles
    every test target in the workspace. Losing F1 would trade the dual-build
    falsifier for the suites instead of adding them."""
    f1 = _workflow()["jobs"].get("f1-test-compile")
    assert f1, "f1-test-compile is gone; the whole-workspace compile falsifier went with it"
    runs = " ".join(s.get("run", "") for s in f1["steps"])
    assert "--workspace" in runs and "--no-run" in runs, runs


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
    print("PASS: the Rust unit suites execute in CI, and a silent suite is not green")
