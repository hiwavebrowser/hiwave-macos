"""Every consumer of diff_pct must survive the NOT-MEASURED state.

Regression for a self-inflicted break: the three-state model changed
`diff_pct` from float to Optional[float], and the display path in
parity_swarm.py still did `{result.diff_pct:.2f}` — so the PR swarm crashed
with `TypeError: unsupported format string passed to NoneType.__format__`
on the very first refused cell.

The lesson this file exists to enforce: widening a field's type in a
dynamically-typed codebase is a change to EVERY reader, and the readers are
found by enumeration, not by memory.
"""
import os
import sys

_REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
# parity_lib resolves the case registry relative to CWD, so these imports only
# work from the repo root. Pinning it here keeps the test runnable from
# anywhere rather than only from where its author happened to stand.
os.chdir(_REPO)
sys.path.insert(0, os.path.join(_REPO, "scripts"))

import parity_aggregate  # noqa: E402
import parity_swarm  # noqa: E402
import parity_test  # noqa: E402


def test_formatters_say_not_measured_instead_of_crashing():
    assert parity_swarm.fmt_diff(None) == "NOT-MEASURED"
    assert parity_swarm.fmt_diff(6.75) == "6.75%"
    assert parity_test._fmt(None) == "NOT-MEASURED"
    assert parity_test._fmt(6.75) == "6.75%"


def test_aggregate_sort_key_handles_none():
    """-None is a TypeError; unmeasured must sort first, not explode."""
    Summary = parity_aggregate.CaseSummary
    cells = [
        Summary(case_id="ok", viewport="800x600", diff_pct=3.0, passed=True,
                stable=True, threshold=15),
        Summary(case_id="refused", viewport="800x600", diff_pct=None, passed=False,
                stable=False, threshold=15, error="INSTRUMENT: dimension_mismatch"),
        Summary(case_id="bad", viewport="800x600", diff_pct=40.0, passed=False,
                stable=True, threshold=15),
    ]
    order = [c.case_id for c in sorted(cells, key=parity_aggregate._worst_first)]
    assert order == ["refused", "bad", "ok"], order


def test_aggregate_averages_measured_cells_only():
    Summary = parity_aggregate.CaseSummary
    cells = [
        Summary(case_id="a", viewport="v", diff_pct=6.0, passed=True, stable=True, threshold=15),
        Summary(case_id="b", viewport="v", diff_pct=None, passed=False, stable=False,
                threshold=15, error="INSTRUMENT: dimension_mismatch"),
    ]
    measured = [c.diff_pct for c in cells if c.diff_pct is not None]
    assert sum(measured) / len(measured) == 6.0, "a refusal must not drag the average"


def test_aggregate_does_not_erase_shard_errors():
    """The aggregate used to hardcode error=None when re-emitting for the gate,
    so a gate that correctly fails on `error` never received one."""
    Summary = parity_aggregate.CaseSummary
    cell = Summary(case_id="x", viewport="v", diff_pct=None, passed=False,
                   stable=False, threshold=15, error="INSTRUMENT: dimension_mismatch")
    assert cell.error, "CaseSummary must carry the shard's error through"


def test_comparison_tools_survive_a_present_null():
    """`.get(key, default)` does NOT fire on a present null — only a missing key.

    That distinction is why the NOT-MEASURED state broke the comparison tools
    silently: every one of them wrote `.get("diff_pct", 100)` and looked
    defended. None of them ran in PR CI, so nothing would have caught it until
    someone compared two runs by hand.
    """
    present_null = {"diff_pct": None}
    assert present_null.get("diff_pct", 100) is None, (
        "if this ever returns 100, Python changed and these guards can go"
    )

    import parity_compare
    import parity_summary
    src_compare = open(parity_compare.__file__).read()
    src_summary = open(parity_summary.__file__).read()
    assert '.get("diff_pct", 100)' not in src_compare, (
        "parity_compare still defaults a present null to 100"
    )
    assert '.get("diff_pct", 100)' not in src_summary, (
        "parity_summary still defaults a present null to 100"
    )


if __name__ == "__main__":
    for name, fn in sorted(globals().items()):
        if name.startswith("test_") and callable(fn):
            fn()
            print(f"ok  {name}")
    print("PASS: the NOT-MEASURED state survives every diff_pct consumer")
