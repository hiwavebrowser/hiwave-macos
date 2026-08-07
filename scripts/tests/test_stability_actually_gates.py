"""Finish-line condition 3 must be able to fail something.

`require_stable` has been True at `pr_merge` and `nightly` since the levels
were written, and until 2026-08-07 it gated NOTHING. parity_gate held only
rows with >= 2 runs to the bar and waived the rest; the PR and nightly scout
phases run each case once; `--primary-viewport-only` then discards the
multi-iteration exploit rows. Every row that reached the gate was a
single-run row and every one was waived. The check was not lenient, it was
unreachable.

The shape of that defect is this campaign's whole subject: an instrument that
reports "nothing wrong" when what it means is "I did not look". These tests
hold three things at once, and all three are needed —

  1. a row the gate cannot judge FAILS (and says so distinctly),
  2. a row it can judge and that IS stable PASSES, so the gate is not merely
     red-by-construction,
  3. the CI lanes that gate on stability actually produce the evidence, so
     (1) does not red-lock every PR forever.

Drop any one and the other two are decoration.

Run: python3 scripts/tests/test_stability_actually_gates.py
"""
import re
import sys
from pathlib import Path

# Resolved, not `dirname(__file__) + ".."` — parity_lib derives REPO_ROOT from
# its own unresolved `__file__`, so an unnormalised entry here makes it look
# for cases/registry.json under scripts/tests/.
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import parity_lib  # noqa: E402
import parity_gate  # noqa: E402
from parity_gate import gate_test_results, level_defaults, measured_runs  # noqa: E402
from parity_aggregate import aggregate_from_results  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = REPO_ROOT / ".github" / "workflows" / "parity.yml"

# Well under every registry threshold, so nothing here can fail on diff and
# every verdict below is attributable to the stability logic alone.
GOOD_DIFF = 0.01


def _row(**over):
    r = {
        "case_id": "card-grid",
        "viewport": "1280x800",
        "diff_pct_median": GOOD_DIFF,
        "stable": True,
        "error": None,
    }
    r.update(over)
    return r


def _gate(rows, require_stable=True, max_variance=0.10):
    return gate_test_results(
        {"results": rows},
        max_diff=25.0,
        require_stable=require_stable,
        max_variance=max_variance,
    )


def _reasons(rows, **kw):
    return [f["reason"] for f in _gate(rows, **kw)["failures"]]


# ---------------------------------------------------------------------------
# 1. The gate can fail
# ---------------------------------------------------------------------------

def test_a_single_run_row_does_not_pass_the_stability_bar():
    """THE defect. This exact row is what the PR aggregate emitted, and the
    old gate returned zero failures for it at pr_merge."""
    row = _row(pixel_runs=1, measured_runs=1, stable=False)
    assert _reasons([row]) == ["stability_unmeasured"], (
        "a case measured once cannot support a stability verdict; passing it "
        "is how condition 3 stayed unenforced for the whole campaign"
    )


def test_a_row_with_no_run_evidence_at_all_fails():
    """Absence of evidence is not evidence of stability.

    The old code defaulted an unlabelled row to `1` and then waived it — so
    the least informative row in the schema got the most generous treatment.
    """
    assert _reasons([_row(stable=True)]) == ["stability_unmeasured"]


def test_three_attempts_with_one_measurement_is_not_three_runs():
    """Attempts are not measurements, and the two must not be conflated.

    A row where two captures errored carries `iterations: 3` but one diff.
    Reading the attempt count would credit it with a full stability sample
    drawn from a single observation.
    """
    row = _row(iterations=3, pixel_runs=3, measured_runs=1, stable=False)
    assert _reasons([row]) == ["stability_unmeasured"], (
        "reason must be stability_unmeasured, not unstable: 'we looked once' "
        "and 'we looked three times and it moved' are different facts"
    )


def test_an_unstable_row_with_full_evidence_fails_as_unstable():
    """The other side of the same distinction — judged, and judged bad."""
    row = _row(measured_runs=3, stable=False, diff_pct_variance=0.9)
    assert _reasons([row]) == ["unstable"]


def test_variance_over_budget_fails_even_when_the_producer_says_stable():
    """The producer's boolean does not get the last word on the gate's budget."""
    row = _row(measured_runs=3, stable=True, diff_pct_variance=0.5)
    assert _reasons([row], max_variance=0.10) == ["variance"]


# ---------------------------------------------------------------------------
# 2. The gate can pass
# ---------------------------------------------------------------------------

def test_three_stable_measured_runs_pass():
    """Guards against 'fixing' the hole by making the gate unconditionally red,
    which is just as uninformative in the other direction."""
    row = _row(measured_runs=3, stable=True, diff_pct_variance=0.01)
    assert _gate([row])["failures"] == []


def test_the_commit_lane_is_not_held_to_the_stability_bar():
    """`commit` fires on every push to master and runs one iteration by design.

    If the tightened bar leaked into a level that does not require stability,
    the push lane would red-lock on rows it was never supposed to judge.
    """
    assert level_defaults("commit")["require_stable"] is False
    assert level_defaults("pr_merge")["require_stable"] is True
    assert level_defaults("nightly")["require_stable"] is True
    assert _gate([_row(pixel_runs=1, stable=False)], require_stable=False)["failures"] == []


# ---------------------------------------------------------------------------
# 3. Schema shear — three producers, three spellings of the same fact
# ---------------------------------------------------------------------------

def test_a_parity_test_shaped_row_is_graded_instead_of_crashing_the_gate():
    """parity_test.py writes `pixel_runs` as a LIST of per-run diffs, where the
    aggregate writes the same key as an INT. `int(<list>)` raised TypeError, so
    pointing parity_gate straight at a parity_test report died on the stability
    branch rather than gating. Latent only because CI reads the aggregate."""
    row = _row(pixel_runs=[GOOD_DIFF, GOOD_DIFF, GOOD_DIFF], stable=True)
    assert _gate([row])["failures"] == []
    short = _row(pixel_runs=[GOOD_DIFF], stable=False)
    assert _reasons([short]) == ["stability_unmeasured"]


def test_the_swarms_iteration_diffs_count_as_evidence():
    """The swarm's own row spells the measured count as `iteration_diffs`."""
    row = _row(iteration_diffs=[GOOD_DIFF, GOOD_DIFF, GOOD_DIFF], stable=True)
    assert _gate([row])["failures"] == []


def test_measured_runs_never_reads_the_attempt_count():
    assert measured_runs({"iterations": 3}) == 0
    assert measured_runs({"pixel_runs": 3}) == 0
    assert measured_runs({"measured_runs": 3}) == 3
    assert measured_runs({"iteration_diffs": [1.0, 2.0]}) == 2
    assert measured_runs({}) == 0


def test_the_aggregate_carries_the_measured_count_to_the_gate():
    """End to end over the real aggregator: a swarm row with three scored
    iterations must arrive at the gate as three, and one with three attempts
    and one score must arrive as one. Without this the tightened gate would
    call every real CI row unmeasured."""
    agg = aggregate_from_results([
        {"case_id": "a", "viewport": "1280x800", "diff_pct_median": GOOD_DIFF,
         "stable": True, "iterations": 3, "iteration_diffs": [0.01, 0.01, 0.01],
         "threshold": 15},
        {"case_id": "b", "viewport": "1280x800", "diff_pct_median": GOOD_DIFF,
         "stable": False, "iterations": 3, "iteration_diffs": [0.01],
         "threshold": 15},
    ])
    by_id = {r["case_id"]: r for r in agg["results"]}
    assert by_id["a"]["measured_runs"] == 3
    assert by_id["b"]["measured_runs"] == 1
    assert [f["reason"] for f in _gate(agg["results"])["failures"]] == ["stability_unmeasured"]


def test_the_cases_only_fallback_keeps_its_evidence():
    """parity_gate's B2 fallback builds rows from `cases[]` when `results[]` is
    missing. If that mapping dropped the count, an otherwise healthy aggregate
    would fail every case as unmeasured."""
    report = {"cases": [{"case_id": "a", "viewport": "1280x800",
                         "diff_pct": GOOD_DIFF, "passed": True, "stable": True,
                         "measured_runs": 3}]}
    assert gate_test_results(report, max_diff=25.0, require_stable=True,
                             max_variance=0.10)["failures"] == []


# ---------------------------------------------------------------------------
# 4. One bar, one number
# ---------------------------------------------------------------------------

def test_the_gate_reads_the_bar_from_parity_lib_rather_than_copying_it():
    """Same rule gate B applies to `aa_tolerance`. Moving the constant must
    move the gate; if it does not, a second number exists somewhere."""
    original = parity_lib.STABILITY_MIN_RUNS
    try:
        parity_lib.STABILITY_MIN_RUNS = 4
        assert parity_gate.stability_min_runs() == 4
        row = _row(measured_runs=3, stable=True, diff_pct_variance=0.01)
        assert _reasons([row]) == ["stability_unmeasured"]
    finally:
        parity_lib.STABILITY_MIN_RUNS = original


def test_the_producer_reads_the_same_bar():
    """parity_lib.aggregate_iterations decides `stable`. It must answer to the
    same constant the gate does, or a row can be published stable and then
    rejected as unmeasured (or worse, the reverse)."""
    from parity_lib import CaseResult, aggregate_iterations

    def _r(i, d=0.01):
        c = CaseResult(case_id="a", case_type="micro", viewport="1280x800",
                       iteration=i, width=1280, height=800, threshold=15.0)
        c.diff_pct = d
        return c

    three = [_r(1), _r(2), _r(3)]
    assert aggregate_iterations(three, max_variance=0.10).stable is True
    original = parity_lib.STABILITY_MIN_RUNS
    try:
        parity_lib.STABILITY_MIN_RUNS = 4
        assert aggregate_iterations(three, max_variance=0.10).stable is False
    finally:
        parity_lib.STABILITY_MIN_RUNS = original


def test_the_bar_is_the_finish_lines_three_iterations():
    assert parity_lib.STABILITY_MIN_RUNS == 3, (
        "finish-line condition 3 is 'stable across 3 iterations' (plan §3.3); "
        "changing this changes the ratified bar, not just a default"
    )


# ---------------------------------------------------------------------------
# 5. The evidence has to be produced, or (1) is a permanent red lock
# ---------------------------------------------------------------------------

def _swarm_invocations():
    """(job name, --iterations value) for every parity_swarm call in the workflow."""
    text = WORKFLOW.read_text()
    jobs = re.split(r"\n  (?=[a-z][a-z0-9-]*:\n)", text)
    found = []
    for block in jobs:
        name = block.strip().split(":", 1)[0].strip()
        for call in re.findall(r"parity_swarm\.py(.*?)(?:\n\n|\n      -)", block, re.S):
            m = re.search(r"--iterations\s+(\d+)", call)
            found.append((name, int(m.group(1)) if m else None))
    return found


def test_every_lane_that_gates_on_stability_runs_the_iterations():
    """The gate asks for evidence; these are the lanes that must supply it.

    A tightened gate without this is not a stricter check, it is a permanent
    red lock — the precise trap the original waiver was written to dodge.
    """
    calls = dict(_swarm_invocations())
    assert "pr-swarm" in calls and "nightly-swarm" in calls, calls
    minimum = parity_lib.STABILITY_MIN_RUNS
    for lane in ("pr-swarm", "nightly-swarm"):
        assert calls[lane] is not None and calls[lane] >= minimum, (
            f"{lane} runs --iterations {calls[lane]}; parity_gate requires "
            f"{minimum} measurements at this level, so every row would fail "
            "as stability_unmeasured"
        )


def test_the_commit_lane_is_left_alone():
    """It does not gate on stability, so tripling it would buy nothing and
    cost every push to master."""
    calls = dict(_swarm_invocations())
    assert calls.get("commit-gate") == 1, calls


def test_the_pr_lane_has_headroom_for_the_extra_iterations():
    text = WORKFLOW.read_text()
    block = text.split("  pr-swarm:", 1)[1].split("\n  pr-aggregate:", 1)[0]
    m = re.search(r"timeout-minutes:\s*(\d+)", block)
    assert m and int(m.group(1)) >= 30, (
        "pr-swarm scout work roughly tripled; a shard killed by the timeout "
        f"is a red PR with no receipt (timeout-minutes: {m and m.group(1)})"
    )


if __name__ == "__main__":
    assert WORKFLOW.exists(), f"no workflow at {WORKFLOW}"
    for name, fn in sorted(globals().items()):
        if name.startswith("test_") and callable(fn):
            fn()
            print(f"ok  {name}")
    print("PASS: stability is enforced on evidence, and the evidence is produced")
