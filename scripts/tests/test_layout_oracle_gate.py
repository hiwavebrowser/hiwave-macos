"""Gate A (geometry) must fail every box it cannot honestly score.

Run: python3 scripts/tests/test_layout_oracle_gate.py

The gate's whole value is that a geometry delta is never rasterizer noise, so
every way it could quietly NOT compare a box is a way it reports a green that
means nothing. The tests below are organised around those ways:

  * a box Chrome captured and RustKit never emitted   -> missing_box
  * two RustKit boxes claiming one selector           -> ambiguous_selector
  * an anonymous or text box near a real element      -> excluded, not paired
  * a box RustKit sized that Chrome collapsed         -> phantom_box
  * a capture that never ran                          -> FAIL, not "0 cases pass"

Plus the join itself, exercised against all 26 committed Chrome baselines
rather than a hand-written fixture: unit tests were fully green on night 1
while the join silently dropped three real elements, and only the corpus
caught it.
"""
import copy
import json
import os
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
from layout_oracle_gate import (  # noqa: E402
    GEOMETRY_TOLERANCE_PX,
    baselines_dir,
    chrome_rects_path,
    compare_case,
    gate_passes,
    load_case_registry,
    run_gate,
)

REPO_ROOT = Path(__file__).resolve().parent.parent.parent


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def rect(x, y, w, h):
    return {"x": x, "y": y, "width": w, "height": h}


def chrome_doc(*elements):
    return {"viewport": {"width": 800, "height": 600}, "elements": list(elements)}


def chrome_el(selector, tag, r):
    return {"selector": selector, "tag": tag, "rect": dict(r)}


def rk_box(selector=None, tag=None, r=None, box_type="block", children=None, **extra):
    box = {"type": box_type, "children": children or []}
    if r is not None:
        box["border_box"] = dict(r)
        # A real export also carries content_rect; the gate must ignore it.
        box["content_rect"] = dict(r)
    if selector is not None:
        box["selector"] = selector
        box["tag"] = tag or selector.rsplit(">", 1)[-1].strip().split(".")[0]
        box["element_id"] = 1
    box.update(extra)
    return box


def rk_doc(*children):
    return {"version": 1, "viewport": {"width": 800, "height": 600},
            "root": rk_box(box_type="block", r=rect(0, 0, 800, 600), children=list(children))}


def synthesize_rustkit_from_chrome(chrome):
    """A perfect RustKit capture: every Chrome element, at Chrome's rect.

    This is the identity case. It must score green on every case, which is what
    makes a later 0.6px perturbation attributable to the perturbation and not
    to the gate mis-joining real corpus data.
    """
    children = [
        rk_box(selector=el["selector"], tag=el["tag"], r=el["rect"])
        for el in chrome["elements"]
    ]
    return rk_doc(*children)


def gate_cases():
    return [
        (cid, case)
        for cid, case in sorted(load_case_registry().items())
        if case["scope"] != "holdout"
    ]


# ---------------------------------------------------------------------------
# The join, against the real corpus
# ---------------------------------------------------------------------------


def test_identity_capture_is_green_on_every_gate_case():
    """All 26 cases, every box compared, zero failures."""
    total_compared = 0
    for case_id, case in gate_cases():
        chrome = json.load(open(chrome_rects_path(case_id, case["scope"])))
        rustkit = synthesize_rustkit_from_chrome(chrome)
        result = compare_case(case_id, chrome, rustkit)
        assert result["green"], (
            f"{case_id}: identity capture scored red: {result['receipts'][:3]}"
        )
        assert result["compared"] == len(chrome["elements"]), (
            f"{case_id}: compared {result['compared']} of {len(chrome['elements'])}"
        )
        total_compared += result["compared"]
    assert total_compared == 1593, (
        f"the gate set is 1593 committed boxes; compared {total_compared}. "
        "A changed count means the corpus or the baselines moved."
    )


def test_one_perturbed_box_produces_exactly_one_receipt():
    """0.6px on one axis of one box, on real data, is one failing line."""
    case_id, case = gate_cases()[0]
    chrome = json.load(open(chrome_rects_path(case_id, case["scope"])))
    rustkit = synthesize_rustkit_from_chrome(chrome)
    target = rustkit["root"]["children"][1]
    target["border_box"]["x"] += 0.6

    result = compare_case(case_id, chrome, rustkit)
    assert not result["green"]
    assert result["geometry_failures"] == 1, result["receipts"]
    assert result["join_failures"] == 0
    line = result["receipts"][0]
    assert line.count(" · ") == 5, f"receipt is not the fixed 6-column form: {line}"
    assert target["selector"] in line
    assert " · x · " in line
    assert "+0.6" in line


# ---------------------------------------------------------------------------
# The tolerance boundary
# ---------------------------------------------------------------------------


def test_exactly_at_tolerance_passes_and_just_over_fails():
    chrome = chrome_doc(chrome_el("body > p", "p", rect(10, 10, 100, 20)))

    at = rk_doc(rk_box("body > p", "p", rect(10 + GEOMETRY_TOLERANCE_PX, 10, 100, 20)))
    assert compare_case("t", chrome, at)["green"], "0.5px is within the bar"

    over = rk_doc(rk_box("body > p", "p", rect(10 + GEOMETRY_TOLERANCE_PX + 0.001, 10, 100, 20)))
    assert not compare_case("t", chrome, over)["green"]


def test_every_axis_is_compared_independently():
    chrome = chrome_doc(chrome_el("body > p", "p", rect(10, 10, 100, 20)))
    rustkit = rk_doc(rk_box("body > p", "p", rect(11, 12, 103, 24)))
    result = compare_case("t", chrome, rustkit)
    axes = {f["axis"] for f in result["failures"]}
    assert axes == {"x", "y", "width", "height"}, axes


def test_the_border_box_is_what_joins_to_chrome():
    """content_rect is inset by padding; comparing it would fail every padded box.

    Chrome's rect is getBoundingClientRect, i.e. the border box. A gate reading
    content_rect reports a constant bogus delta on real pages, which reads as a
    layout bug and sends the night's dig at the wrong thing.
    """
    chrome = chrome_doc(chrome_el("body > div", "div", rect(0, 0, 100, 100)))
    box = rk_box("body > div", "div", rect(0, 0, 100, 100))
    box["content_rect"] = rect(20, 20, 60, 60)  # padding 20 all round
    assert compare_case("t", chrome, rk_doc(box))["green"]


# ---------------------------------------------------------------------------
# The ways a box could go unscored
# ---------------------------------------------------------------------------


def test_a_box_rustkit_never_emitted_is_a_failure():
    chrome = chrome_doc(
        chrome_el("body > p", "p", rect(0, 0, 10, 10)),
        chrome_el("body > span", "span", rect(0, 0, 10, 10)),
    )
    result = compare_case("t", chrome, rk_doc(rk_box("body > p", "p", rect(0, 0, 10, 10))))
    kinds = [f["kind"] for f in result["failures"]]
    assert kinds == ["missing_box"], kinds
    assert result["compared"] == 1, "the box that WAS present must still be scored"


def test_a_duplicate_selector_is_reported_not_first_matched():
    """Two boxes claiming one selector: scoring either one is a coin flip.

    First-matching would score the correct twin and call the case green while a
    second, wrong box sat unexamined in the tree.
    """
    chrome = chrome_doc(chrome_el("body > p", "p", rect(0, 0, 10, 10)))
    rustkit = rk_doc(
        rk_box("body > p", "p", rect(0, 0, 10, 10)),      # correct
        rk_box("body > p", "p", rect(0, 0, 999, 999)),    # wrong
    )
    result = compare_case("t", chrome, rustkit)
    assert not result["green"]
    assert [f["kind"] for f in result["failures"]] == ["ambiguous_selector"]
    assert result["compared"] == 0, "an ambiguous selector must not be scored at all"


def test_anonymous_and_text_boxes_are_excluded_never_paired():
    """The Option on identity is load-bearing.

    Here an anonymous box and a text box sit at the wrong place in the tree,
    adjacent to the one real element. Pairing either of them positionally would
    manufacture a geometry failure on a case that is actually correct.
    """
    chrome = chrome_doc(chrome_el("body > p", "p", rect(10, 10, 100, 20)))
    rustkit = rk_doc(
        rk_box(box_type="anonymous_block", r=rect(500, 500, 3, 3)),
        {"type": "text", "text": "hello", "rect": rect(600, 600, 7, 7), "children": []},
        rk_box("body > p", "p", rect(10, 10, 100, 20)),
    )
    result = compare_case("t", chrome, rustkit)
    assert result["green"], result["receipts"]
    assert result["compared"] == 1
    assert result["rustkit_identified"] == 1
    assert result["rustkit_boxes"] == 4, "the unidentified boxes are still counted"


def test_a_box_chrome_would_have_captured_but_did_not_is_a_phantom():
    """RustKit gave size to something Chrome collapsed to zero."""
    chrome = chrome_doc(chrome_el("body > p", "p", rect(0, 0, 10, 10)))
    rustkit = rk_doc(
        rk_box("body > p", "p", rect(0, 0, 10, 10)),
        rk_box("body > div.ghost", "div", rect(0, 0, 200, 50)),
    )
    result = compare_case("t", chrome, rustkit)
    assert not result["green"]
    assert [f["kind"] for f in result["failures"]] == ["phantom_box"]


def test_chromes_own_omissions_are_not_phantoms():
    """Mirror capture_baseline.mjs, or the gate invents failures on every page.

    Chrome drops zero-size elements and a fixed tag list before writing
    layout-rects.json. RustKit emits both. Neither is a defect.
    """
    chrome = chrome_doc(chrome_el("body > p", "p", rect(0, 0, 10, 10)))
    rustkit = rk_doc(
        rk_box("body > p", "p", rect(0, 0, 10, 10)),
        rk_box("html", "html", rect(0, 0, 800, 600)),          # skipped tag
        rk_box("body > style", "style", rect(0, 0, 800, 20)),  # skipped tag
        rk_box("body > i.empty", "i", rect(40, 40, 0, 0)),     # zero-size
    )
    result = compare_case("t", chrome, rustkit)
    assert result["green"], result["receipts"]


# ---------------------------------------------------------------------------
# A run that measured nothing
# ---------------------------------------------------------------------------


def test_a_run_with_no_captures_fails_rather_than_reporting_all_green():
    """"PASS: all 0 cases" is how a broken pipeline turns green."""
    with tempfile.TemporaryDirectory() as empty:
        report = run_gate(Path(empty))
        assert report["summary"]["measured"] == 0
        assert report["summary"]["unmeasured"] == len(gate_cases())
        assert not gate_passes(report)
        assert all(not c["green"] for c in report["cases"])


def test_a_single_missing_capture_does_not_pass_by_omission():
    """One case captured perfectly, one never captured, is not a green run."""
    cases = gate_cases()
    present, absent = cases[0][0], cases[1][0]
    with tempfile.TemporaryDirectory() as root:
        chrome = json.load(open(chrome_rects_path(present, cases[0][1]["scope"])))
        out = Path(root) / present
        out.mkdir(parents=True)
        with open(out / "layout.json", "w") as handle:
            json.dump(synthesize_rustkit_from_chrome(chrome), handle)

        report = run_gate(Path(root), case_ids=[present, absent])
        assert report["summary"]["measured"] == 1
        assert report["summary"]["green"] == 1
        assert not gate_passes(report)
        missing = [c for c in report["cases"] if c["case_id"] == absent][0]
        assert missing["reason"] == "no_rustkit_capture"


def test_the_holdout_scope_does_not_gate():
    """Canary-only until the 26 are green (plan §3.6)."""
    with tempfile.TemporaryDirectory() as empty:
        gating = run_gate(Path(empty))
        with_holdout = run_gate(Path(empty), include_non_gating=True)
    assert gating["summary"]["total_cases"] == 26
    assert with_holdout["summary"]["total_cases"] == 32


if __name__ == "__main__":
    assert baselines_dir().exists(), f"no baselines at {baselines_dir()}"
    for name, fn in sorted(globals().items()):
        if name.startswith("test_") and callable(fn):
            fn()
            print(f"ok  {name}")
    print("PASS: Gate A fails every box it cannot honestly score")
