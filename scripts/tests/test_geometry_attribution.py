"""The attribution board must not flatter the work it aims.

Run: python3 scripts/tests/test_geometry_attribution.py

This board exists to say WHICH failing box is failing at its own edge, and it
has two ways to lie that Gate A does not:

  * calling a box a ROOT when its parent already moved it that far — which
    aims a night at a defect that is three ancestors up
  * calling a box FONT-INDEPENDENT when a text measurement can still reach it —
    which aims a night at a defect a text-less seat cannot score at all

The second one is not hypothetical: the first version of this classifier looked
only INSIDE each box, called `rounded-corners`' inline-block staircase
font-independent, and put 170 boxes on the board where 13 belong. The rest were
collapsed source newlines between the divs.

And, as everywhere in this campaign, a board that measured nothing is not a
clean board.
"""
import json
import os
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
from geometry_attribution import (  # noqa: E402
    annotate,
    attribute_case,
    board_ran,
    run_attribution,
)
import geometry_attribution  # noqa: E402
import layout_oracle_gate  # noqa: E402


def rect(x=0.0, y=0.0, width=10.0, height=10.0):
    return {"x": x, "y": y, "width": width, "height": height}


def chrome_doc(*pairs):
    return {"elements": [{"selector": s, "rect": r} for s, r in pairs]}


def box(selector, r, children=(), **extra):
    node = {"selector": selector, "border_box": dict(r), "children": list(children)}
    node.update(extra)
    return node


def text(value=" "):
    return {"type": "text", "text": value, "children": []}


# ---------------------------------------------------------------------------
# root vs carried
# ---------------------------------------------------------------------------


def test_a_child_moving_exactly_with_its_parent_is_carried_not_a_root():
    """flex-positioning `#check1`: 8px out, and laid out perfectly in its row."""
    chrome = chrome_doc(
        ("div.row", rect(y=197.0)),
        ("#check1", rect(y=207.0)),
    )
    rustkit = box("div.row", rect(y=189.0), [box("#check1", rect(y=199.0))])

    result = attribute_case("c", chrome, rustkit)
    by_selector = {f["selector"]: f for f in result["findings"]}
    assert by_selector["div.row"]["root"] is True, "the row itself IS a root"
    assert by_selector["#check1"]["root"] is False, "the checkbox is carried"
    assert abs(by_selector["#check1"]["residual"]) < 1e-6
    assert result["roots"] == 1 and result["carried"] == 1


def test_a_child_whose_parent_is_exact_is_a_root():
    chrome = chrome_doc(("div.row", rect(y=100.0)), ("span.a", rect(y=110.0)))
    rustkit = box("div.row", rect(y=100.0), [box("span.a", rect(y=118.0))])

    result = attribute_case("c", chrome, rustkit)
    finding = next(f for f in result["findings"] if f["selector"] == "span.a")
    assert finding["root"] is True
    assert abs(finding["residual"] - 8.0) < 1e-6


def test_each_axis_is_attributed_on_its_own():
    """A box can be carried on one axis and a root on another.

    Collapsing to one verdict per BOX would hide exactly the case the grind
    needs: a correctly-positioned box that is the wrong size, under a parent
    that is itself displaced.
    """
    chrome = chrome_doc(
        ("div.row", rect(x=0.0, y=100.0, width=200.0, height=50.0)),
        ("span.a", rect(x=10.0, y=110.0, width=100.0, height=20.0)),
    )
    rustkit = box(
        "div.row",
        rect(x=0.0, y=90.0, width=200.0, height=50.0),
        [box("span.a", rect(x=10.0, y=100.0, width=140.0, height=20.0))],
    )

    result = attribute_case("c", chrome, rustkit)
    by_axis = {f["axis"]: f for f in result["findings"] if f["selector"] == "span.a"}
    assert by_axis["y"]["root"] is False, "y moved exactly with the parent"
    assert by_axis["width"]["root"] is True, "width is 40px wrong at its own edge"


def test_a_box_with_no_comparable_ancestor_owns_its_whole_delta():
    """`html > body` has nothing above it, so nothing can be subtracted."""
    chrome = chrome_doc(("html > body", rect(height=1342.0)))
    rustkit = box("html > body", rect(height=1279.0))

    result = attribute_case("c", chrome, rustkit)
    finding = next(f for f in result["findings"] if f["axis"] == "height")
    assert finding["anchor"] is None
    assert finding["root"] is True
    assert abs(finding["residual"] - -63.0) < 1e-6


def test_the_anchor_skips_an_ancestor_chrome_never_captured():
    """Chrome's capture drops zero-size elements and a skip list of tags.

    An ancestor present on only one side cannot supply a delta, so the search
    must continue upward. Stopping there would treat every descendant of a
    wrapper Chrome omitted as its own root.
    """
    chrome = chrome_doc(("div.outer", rect(y=100.0)), ("span.a", rect(y=100.0)))
    rustkit = box(
        "div.outer",
        rect(y=110.0),
        [box("div.invisible", rect(y=110.0), [box("span.a", rect(y=110.0))])],
    )

    result = attribute_case("c", chrome, rustkit)
    finding = next(f for f in result["findings"] if f["selector"] == "span.a")
    assert finding["anchor"] == "div.outer"
    assert finding["root"] is False


def test_the_tolerance_is_gate_as_not_a_second_number():
    """Two tolerances that must agree, written down twice, will disagree.

    Asserted on the BEHAVIOUR, not on the name: the first version of this test
    checked that the module-level constant followed Gate A's and stayed green
    when the default argument was hardcoded to 0.5 — a guard satisfied by the
    import line while the code that uses it had its own number. Moving Gate A's
    constant must move what this board calls a failure.
    """
    import importlib

    original = layout_oracle_gate.GEOMETRY_TOLERANCE_PX
    chrome = chrome_doc(("div.a", rect(y=100.0)))
    rustkit = box("div.a", rect(y=110.0))
    try:
        layout_oracle_gate.GEOMETRY_TOLERANCE_PX = 50.0
        importlib.reload(geometry_attribution)
        widened = geometry_attribution.attribute_case("c", chrome, rustkit)
        assert widened["failing_axes"] == 0, (
            "a 10px delta is inside a 50px tolerance — the board must take "
            "Gate A's constant, not carry its own"
        )
    finally:
        layout_oracle_gate.GEOMETRY_TOLERANCE_PX = original
        importlib.reload(geometry_attribution)

    restored = geometry_attribution.attribute_case("c", chrome, rustkit)
    assert restored["failing_axes"] == 1
    assert geometry_attribution.GEOMETRY_TOLERANCE_PX == original


# ---------------------------------------------------------------------------
# text-reachable vs font-independent
# ---------------------------------------------------------------------------


def test_a_whitespace_only_sibling_taints_the_box_next_to_it():
    """THE measurement that changed this board from 170 boxes to 13.

    `rounded-corners` lays inline-block divs with nothing but source newlines
    between them. Each collapsed space is a glyph with an advance, and a seat
    with no font backend measures it at 8.0 where Chrome measures 4.1875. The
    boxes are empty; their positions are not font-independent.
    """
    chrome = chrome_doc(("div.a", rect(x=10.0)), ("div.b", rect(x=110.0)))
    rustkit = box(
        "root",
        rect(),
        [box("div.a", rect(x=10.0)), text("\n      "), box("div.b", rect(x=130.0))],
    )

    _, _, reachable = annotate(rustkit)
    assert reachable["div.b"] is True, "a collapsed space moves the box after it"
    assert reachable["div.a"] is True, "line breaking moves the box before it too"

    result = attribute_case("c", chrome, rustkit)
    assert result["roots"] == 1
    assert result["font_independent_roots"] == 0, (
        "an inline-block staircase is not work a text-less seat can aim at"
    )


def test_text_in_the_subtree_taints_the_ancestor():
    chrome = chrome_doc(("div.card", rect(height=100.0)))
    rustkit = box("div.card", rect(height=120.0), [box("p", rect(), [text("hello")])])

    result = attribute_case("c", chrome, rustkit)
    assert result["roots"] == 1
    assert result["font_independent_roots"] == 0


def test_a_control_sized_from_a_label_is_text_reachable_and_a_bare_checkbox_is_not():
    """A `<button>`'s label is not a child text node — it rides in control_type.

    Reading only the tree would call every button font-independent, and button
    width is measured text. A checkbox has no such string, which is why
    `#check1` is one of the few boxes this seat can honestly score.
    """
    rustkit = box(
        "form",
        rect(),
        [
            box("button.save", rect(), control_type='Button { label: "Save" }'),
            box("#check1", rect(), control_type="Checkbox { checked: false }"),
        ],
    )

    _, _, reachable = annotate(rustkit)
    assert reachable["button.save"] is True
    assert reachable["#check1"] is False


def test_font_independent_roots_are_a_subset_of_roots():
    """A carried box is never counted as aimable work, however text-free."""
    chrome = chrome_doc(("div.row", rect(y=100.0)), ("div.inner", rect(y=100.0)))
    rustkit = box("div.row", rect(y=140.0), [box("div.inner", rect(y=140.0))])

    result = attribute_case("c", chrome, rustkit)
    assert result["roots"] == 1
    assert result["font_independent_roots"] == 1, "the row is the root, and it is text-free"
    assert result["carried"] == 1


# ---------------------------------------------------------------------------
# non-gating, and the blank-row rule
# ---------------------------------------------------------------------------


def test_a_board_that_measured_nothing_is_not_a_clean_board():
    with tempfile.TemporaryDirectory() as empty:
        report = run_attribution(Path(empty))
    assert report["summary"]["measured"] == 0
    assert not board_ran(report), "no captures is DID NOT RUN, not zero defects"
    assert all(not c["measured"] for c in report["cases"])
    assert all(c["roots"] == 0 for c in report["cases"])


def test_catastrophic_numbers_still_publish():
    """Non-gating means the NUMBERS cannot fail a PR — not that it always
    exits 0. A board that stopped being produced would otherwise look exactly
    like a board with nothing to report."""
    chrome = chrome_doc(("html > body", rect(height=1000.0)))
    rustkit = box("html > body", rect(height=1.0))
    result = attribute_case("c", chrome, rustkit)
    report = {
        "cases": [result],
        "summary": {
            "measured": 1,
            "total_cases": 1,
            "unmeasured": 0,
            "failing_axes": result["failing_axes"],
            "roots": result["roots"],
            "carried": result["carried"],
            "font_independent_roots": result["font_independent_roots"],
        },
    }
    assert result["roots"] >= 1
    assert board_ran(report) is True


def test_the_holdout_scope_is_excluded_by_default():
    with tempfile.TemporaryDirectory() as empty:
        gating = run_attribution(Path(empty))
        with_holdout = run_attribution(Path(empty), include_non_gating=True)
    assert gating["summary"]["total_cases"] == 26
    assert with_holdout["summary"]["total_cases"] == 32


def test_an_unmeasured_case_reports_a_reason_rather_than_a_zero():
    with tempfile.TemporaryDirectory() as empty:
        report = run_attribution(Path(empty), case_ids=["bg-pure"])
    case = report["cases"][0]
    assert case["measured"] is False
    assert case["reason"], "a case that could not be scored must say why"


def test_the_report_is_json_serialisable():
    """It is published as an artifact; a report that cannot be written is not
    a report."""
    chrome = chrome_doc(("div.a", rect(y=10.0)))
    rustkit = box("div.a", rect(y=30.0))
    json.dumps(attribute_case("c", chrome, rustkit))


if __name__ == "__main__":
    for name, fn in sorted(globals().items()):
        if name.startswith("test_") and callable(fn):
            fn()
            print(f"ok  {name}")
    print("PASS: the attribution board does not flatter the work it aims")
