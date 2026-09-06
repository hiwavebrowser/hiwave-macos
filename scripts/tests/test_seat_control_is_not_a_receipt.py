"""The seat-control report is a diagnostic. It must never become a receipt.

`scripts/seat_control_report.py` exists to answer one question on a non-macOS
seat: *is this geometry delta a RustKit defect, or is it the seat?* It answers
it by capturing Chrome on the seat itself and comparing three ways.

That makes it the most dangerous kind of instrument this campaign has built,
because its output looks exactly like Gate A's and is not one. Two failure
modes and one whole category of lie:

  * cited as a parity number — a seat-control figure is not macOS and not an
    `N/26`; the campaign's whole thesis is that a number taken with a broken
    instrument is worse than no number (plan §1);
  * used while stale or partial — a control captured from a different fixture,
    or missing the case being scored, still produces numbers that parse. They
    are about a page that no longer exists;
  * calling a real defect "the seat" — the one attribution error that would
    make the trench SKIP real work, which is worse than the noise it removes.

Every guard below was mutation-checked: the fix was removed, the guard was
confirmed RED, and the fix restored. A guard that stays green without its fix
is decoration.

Run: python3 scripts/tests/test_seat_control_is_not_a_receipt.py
"""
import hashlib
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
import seat_control_report as scr  # noqa: E402

REPO = Path(__file__).resolve().parent.parent.parent


def _rects(elements):
    return {
        "viewport": {"width": 100, "height": 100},
        "elementCount": len(elements),
        "elements": [{"selector": s, "rect": r} for s, r in elements.items()],
    }


def _box(x=0.0, y=0.0, width=10.0, height=10.0):
    return {"x": x, "y": y, "width": width, "height": height}


def _write_case(root, scope, case_id, elements):
    d = root / scope / case_id
    d.mkdir(parents=True, exist_ok=True)
    (d / "layout-rects.json").write_text(json.dumps(_rects(elements)))


def _stamp(root, fixtures):
    root.mkdir(parents=True, exist_ok=True)
    (root / "STAMP.json").write_text(
        json.dumps(
            {
                "kind": "seat-control",
                "not_a_receipt": True,
                "captured_at": "2026-09-06T00:00:00Z",
                "platform": "linux-x64",
                "cases": sorted(fixtures),
                "fixtures": fixtures,
                "font_resolution": {"Georgia": "/fake/DejaVuSerif.ttf"},
            }
        )
    )


def _layout(root, case_id, elements):
    d = root / case_id
    d.mkdir(parents=True, exist_ok=True)
    children = [{"selector": s, "border_box": r, "children": []} for s, r in elements.items()]
    (d / "layout.json").write_text(json.dumps({"root": {"children": children}}))


# ---------------------------------------------------------------------------
# The attribution itself
# ---------------------------------------------------------------------------


def test_a_delta_that_survives_the_control_is_never_blamed_on_the_seat():
    """The sharp case, and the only one that can cost the campaign real work.

    An element 40px out of place against the pinned baseline AND 40px out
    against the seat's own Chrome is a RustKit defect. Attributing it to the
    platform would tell the trench to skip it — a silence that looks exactly
    like the absence of a bug.
    """
    assert scr.classify(reported=40.0, real=40.0) == "real"
    assert scr.classify(reported=-40.0, real=-40.0) == "real"
    # Both fail but by different amounts: still real, and the reported
    # magnitude is not the defect's magnitude.
    assert scr.classify(reported=40.0, real=12.0) == "mixed"
    for reported, real in ((40.0, 40.0), (40.0, 12.0), (-9.0, -3.0)):
        assert scr.classify(reported, real) != "confound", (
            f"classify({reported}, {real}) called a surviving delta 'confound' — "
            "that tells the trench a real defect is platform noise"
        )


def test_only_a_delta_the_control_clears_is_the_seat():
    """Fails against macOS Chrome, passes against the seat's own Chrome."""
    assert scr.classify(reported=40.0, real=0.0) == "confound"
    assert scr.classify(reported=40.0, real=0.25) == "confound"


def test_an_error_the_platform_hides_is_reported_not_dropped():
    """`masked`: green on the receipt oracle, wrong against the control.

    RustKit's error and the platform's error cancel, so Gate A on this seat
    sees nothing. Dropping the category would make the report claim more
    safety than it has — the compensating-error pattern this campaign has now
    found three times, as a category rather than an anecdote.
    """
    assert scr.classify(reported=0.0, real=40.0) == "masked"
    assert scr.classify(reported=0.1, real=-8.0) == "masked"


def test_agreement_on_both_oracles_is_green():
    assert scr.classify(reported=0.0, real=0.0) == "green"
    assert scr.classify(reported=0.4, real=-0.4) == "green"


def test_tolerance_comes_from_the_gate_and_is_not_restated():
    """One tolerance, defined once.

    A report that carried its own copy could drift into disagreeing with Gate A
    about what a failure is, and the disagreement would be invisible: both
    would keep printing plausible numbers.
    """
    import layout_oracle_gate

    assert scr.GEOMETRY_TOLERANCE_PX is layout_oracle_gate.GEOMETRY_TOLERANCE_PX
    assert scr.NON_GATING_SCOPES is layout_oracle_gate.NON_GATING_SCOPES


# ---------------------------------------------------------------------------
# Refusals — a report that cannot measure must say so, never assume zero
# ---------------------------------------------------------------------------


def _score_in_synthetic_world(tmp, *, stamp_fixtures, pinned, control, rustkit):
    """Score one case in a world where NOTHING else can produce UNMEASURED.

    Pinned baseline, control capture, fixture file and RustKit dump all exist
    and agree. Written this way on purpose: the first draft of the
    missing-stamp-entry guard below passed while its fix was mutated away,
    because the case reached an earlier refusal — no control file, then no
    pinned baseline — and never touched the line it was written for. A guard
    satisfied by a different guard is decoration.
    """
    fixture = REPO / "cases" / "registry.json"
    control_dir, layout_dir = tmp / "control", tmp / "layout"
    _stamp(control_dir, stamp_fixtures)
    _write_case(control_dir, "websuite", "probe", control)
    pinned_dir = tmp / "baselines" / "pinned" / "websuite" / "probe"
    pinned_dir.mkdir(parents=True, exist_ok=True)
    (pinned_dir / "layout-rects.json").write_text(json.dumps(_rects(pinned)))
    _layout(layout_dir, "probe", rustkit)
    (tmp / "cases").mkdir(exist_ok=True)
    (tmp / "cases" / "registry.json").write_bytes(fixture.read_bytes())

    original = scr.PINNED_SET, scr.REPO_ROOT
    try:
        scr.PINNED_SET = "pinned"
        scr.REPO_ROOT = tmp
        return scr.score_case(
            "probe", "websuite", layout_dir, control_dir, "cases/registry.json",
            json.loads((control_dir / "STAMP.json").read_text()),
        )
    finally:
        scr.PINNED_SET, scr.REPO_ROOT = original


def test_a_case_the_stamp_does_not_cover_is_unmeasured_not_zero_confound():
    """The dangerous default: an unvouched-for control reads as 'no confound'.

    Zero confound is exactly what a healthy case looks like, so this is the
    most attractive wrong answer in the file. The control data here is present
    and well formed — only the stamp does not vouch for it, which is what a
    control copied between machines, or left behind by an older capture, looks
    like.
    """
    fixture_sha = hashlib.sha256((REPO / "cases" / "registry.json").read_bytes()).hexdigest()
    with tempfile.TemporaryDirectory() as tmp:
        rec = _score_in_synthetic_world(
            Path(tmp),
            stamp_fixtures={"some-other-case": fixture_sha},
            pinned={"a": _box()},
            control={"a": _box()},
            rustkit={"a": _box(x=40.0)},
        )
    assert rec["status"] == "UNMEASURED", (
        f"a case the stamp does not cover scored {rec['status']} — an "
        "unvouched-for control must never be reported as zero confound"
    )
    assert "confound_sum" not in rec

    # And the world itself is sound: with the stamp entry present the same
    # inputs measure. Without this the test above would pass on a broken world.
    with tempfile.TemporaryDirectory() as tmp:
        ok = _score_in_synthetic_world(
            Path(tmp),
            stamp_fixtures={"probe": fixture_sha},
            pinned={"a": _box()},
            control={"a": _box()},
            rustkit={"a": _box(x=40.0)},
        )
    assert ok["status"] == "MEASURED", ok


def test_a_control_captured_from_a_different_fixture_is_refused():
    """Staleness. The numbers still parse; they are about a page that changed."""
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        control, layout = tmp / "control", tmp / "layout"
        _stamp(control, {"bg-solid": "0" * 64})  # not any real fixture's hash
        _write_case(control, "websuite", "bg-solid", {"body": _box()})
        _layout(layout, "bg-solid", {"body": _box()})

        rec = scr.score_case(
            "bg-solid", "websuite", layout, control, "cases/registry.json",
            json.loads((control / "STAMP.json").read_text()),
        )
    assert rec["status"] == "UNMEASURED"
    assert "fixture changed" in rec["reason"], rec["reason"]


def test_a_control_missing_elements_is_refused_not_partially_scored():
    """A partial control under-reports the confound exactly where it is blind.

    Scoring the intersection would license working a root that is pure platform
    noise, which is the failure this whole script exists to prevent.
    """
    fixture = REPO / "cases" / "registry.json"
    sha = hashlib.sha256(fixture.read_bytes()).hexdigest()
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        control, layout = tmp / "control", tmp / "layout"
        pinned = tmp / "pinned"
        _stamp(control, {"probe": sha})
        _write_case(control, "websuite", "probe", {"a": _box()})
        _write_case(pinned, "websuite", "probe", {"a": _box(), "b": _box()})
        _layout(layout, "probe", {"a": _box(), "b": _box()})

        # The fixture has to live under the patched root too, or the staleness
        # guard fires first and this test passes for the wrong reason.
        (tmp / "cases").mkdir(exist_ok=True)
        (tmp / "cases" / "registry.json").write_bytes(fixture.read_bytes())

        original = scr.PINNED_SET, scr.REPO_ROOT
        try:
            scr.PINNED_SET = "pinned"
            scr.REPO_ROOT = tmp
            (tmp / "baselines").mkdir(exist_ok=True)
            os.rename(tmp / "pinned", tmp / "baselines" / "pinned")
            rec = scr.score_case(
                "probe", "websuite", layout, control, "cases/registry.json",
                json.loads((control / "STAMP.json").read_text()),
            )
        finally:
            scr.PINNED_SET, scr.REPO_ROOT = original

    assert rec["status"] == "UNMEASURED", (
        f"a control missing 1 of 2 elements scored {rec['status']} — a partial "
        "control silently under-reports the confound on what it lacks"
    )
    assert "selector sets disagree" in rec["reason"], rec["reason"]


def test_no_stamp_refuses_the_whole_run():
    with tempfile.TemporaryDirectory() as tmp:
        try:
            scr.load_control_stamp(Path(tmp))
        except scr.ControlUnusable:
            return
    raise AssertionError("a missing seat control must refuse, not run on nothing")


def test_a_foreign_stamp_is_refused():
    """Only a seat control may be used as one — not the pinned set, not a copy."""
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        (tmp / "STAMP.json").write_text(json.dumps({"kind": "chrome-148", "cases": []}))
        try:
            scr.load_control_stamp(tmp)
        except scr.ControlUnusable:
            return
    raise AssertionError("a stamp that is not a seat control must be refused")


# ---------------------------------------------------------------------------
# It must not be able to become a receipt
# ---------------------------------------------------------------------------


def test_a_measured_record_carries_no_verdict_a_reader_could_quote():
    """Checked on the record the report actually emits, not on its prose.

    The receipt is `scripts/finish_line_receipt.py` against the pinned macOS
    set. This report scores three comparisons and stops: if a record ever grows
    a pass/green/verdict field, someone will read a seat number as an `N/26`.
    """
    fixture = REPO / "cases" / "registry.json"
    sha = hashlib.sha256(fixture.read_bytes()).hexdigest()
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        control, layout = tmp / "control", tmp / "layout"
        _stamp(control, {"probe": sha})
        _write_case(control, "websuite", "probe", {"a": _box(x=1.0)})
        (tmp / "baselines" / "pinned" / "websuite" / "probe").mkdir(parents=True)
        (tmp / "baselines" / "pinned" / "websuite" / "probe" / "layout-rects.json").write_text(
            json.dumps(_rects({"a": _box()}))
        )
        _layout(layout, "probe", {"a": _box(x=9.0)})
        (tmp / "cases").mkdir(exist_ok=True)
        (tmp / "cases" / "registry.json").write_bytes(fixture.read_bytes())

        original = scr.PINNED_SET, scr.REPO_ROOT
        try:
            scr.PINNED_SET = "pinned"
            scr.REPO_ROOT = tmp
            rec = scr.score_case(
                "probe", "websuite", layout, control, "cases/registry.json",
                json.loads((control / "STAMP.json").read_text()),
            )
        finally:
            scr.PINNED_SET, scr.REPO_ROOT = original

    assert rec["status"] == "MEASURED", rec
    banned = {"green", "passed", "pass", "verdict", "finish_line", "n_of_26", "score"}
    leaked = banned & {k.lower() for k in rec}
    assert not leaked, (
        f"a seat-control record grew {sorted(leaked)} — a diagnostic that "
        "publishes a verdict will be quoted as a parity receipt"
    )
    # It still has to do its job: the delta survives the control, so it is real.
    assert rec["buckets"]["confound"] == 0 and rec["real_sum"] > 0


def test_every_output_path_is_labelled_not_a_receipt():
    """Both the human board and the JSON carry the label, not just the docstring."""
    source = (REPO / "scripts" / "seat_control_report.py").read_text()
    assert 'print("seat-control confound report — NOT A RECEIPT' in source, (
        "the printed board lost its NOT A RECEIPT header"
    )
    assert '"not_a_receipt": True' in source, "the JSON report lost its not_a_receipt flag"


def test_seat_control_output_cannot_be_committed():
    """A committed seat control would sit next to `chrome-148` and be trusted.

    The gate resolves a baseline set by directory name (PARITY_BASELINE_SET),
    so a control that reaches the repo is one env var away from being scored
    as the receipt.
    """
    ignored = (REPO / ".gitignore").read_text()
    assert "baselines/seat-control/" in ignored, (
        "baselines/seat-control/ is not gitignored — a diagnostic baseline "
        "must never be committable alongside the pinned macOS set"
    )
    result = subprocess.run(
        ["git", "check-ignore", "-q", "baselines/seat-control/websuite/x/layout-rects.json"],
        cwd=REPO,
        capture_output=True,
    )
    assert result.returncode == 0, "git does not actually ignore baselines/seat-control/"


def test_a_report_that_measured_nothing_is_a_failure():
    """Unmeasured is never green — the rule the gates already hold to."""
    source = (REPO / "scripts" / "seat_control_report.py").read_text()
    assert "return 0 if measured else 1" in source, (
        "a report that measured nothing must exit 1, not 0"
    )


if __name__ == "__main__":
    failures = 0
    for name, fn in sorted(globals().items()):
        if name.startswith("test_") and callable(fn):
            try:
                fn()
                print(f"PASS {name}")
            except Exception as exc:  # noqa: BLE001
                failures += 1
                print(f"FAIL {name}: {exc}")
    print(f"\n{failures} failure(s)")
    sys.exit(1 if failures else 0)
