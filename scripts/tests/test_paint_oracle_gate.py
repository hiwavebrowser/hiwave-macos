"""Gate B (paint) must fail structural bugs the percentage would forgive.

Run: python3 scripts/tests/test_paint_oracle_gate.py

Gate A's value is that a geometry delta is never noise. Gate B's value is the
opposite shape: most paint deltas ARE noise, so the percentage half is
deliberately generous — and everything that matters therefore rides on the
discrete half, which auto-fails regardless of percentage. The tests are
organised around the ways that second half could quietly not fire:

  * the pinned tolerance read from somewhere other than the policy file
  * a wrong solid fill too small to move the percentage      -> auto-fail
  * an unclipped rounded corner, likewise                    -> auto-fail
  * a detector that fires when it cannot attribute           -> must decline
  * a frame that never rendered                              -> FAIL, not skip

The two discrete detectors are exercised against the committed Chrome
baselines, not hand-drawn fixtures: the defect is injected into a real capture
and the gate must name the real selector. Night 1's lesson was that unit tests
stayed green while the join dropped three real elements, and only the corpus
caught it.
"""
import copy
import json
import os
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
from parity_image import Image, read_png  # noqa: E402
from paint_oracle_gate import (  # noqa: E402
    MIN_NOTCH_PX,
    PAINT_PASS_FRACTION,
    PolicyError,
    compare_case,
    corners,
    count_outside_tolerance,
    flat_color,
    gate_passes,
    load_aa_tolerance,
    load_case_registry,
    load_styles,
    parse_radius,
    px_box,
    run_gate,
)

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
BASELINES = REPO_ROOT / "baselines" / "chrome-148"
TOLERANCE = load_aa_tolerance()


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def solid(width, height, color):
    return Image(width, height, bytes(color) * (width * height))


def with_pixels(image, pixels, color):
    """A copy of `image` with `pixels` repainted — the injected defect."""
    buffer = bytearray(image.rgb)
    for x, y in pixels:
        i = (y * image.width + x) * 3
        buffer[i], buffer[i + 1], buffer[i + 2] = color
    return Image(image.width, image.height, bytes(buffer))


def load_case(case_id):
    case = load_case_registry()[case_id]
    base = BASELINES / case["scope"] / case_id
    return (
        read_png(base / "baseline.png"),
        json.loads((base / "layout-rects.json").read_text())["elements"],
        load_styles(base / "computed-styles.json"),
    )


def write_ppm(path, image):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(
        b"P6\n%d %d\n255\n" % (image.width, image.height) + image.rgb
    )


def score(case_id, chrome, rustkit, elements, styles):
    return compare_case(case_id, chrome, rustkit, elements, styles, TOLERANCE)


# ---------------------------------------------------------------------------
# The pinned constant
# ---------------------------------------------------------------------------


def policy_text(default, sections):
    out = ["# Visual Diff Policy", "", "### Default Policy", "", "```json",
           '{ "aa_tolerance": %d }' % default, "```", ""]
    for heading, value in sections:
        out += [f"### {heading}", "", "```json",
                '{ "aa_tolerance": %d }' % value, "```", ""]
    return "\n".join(out)


def temp_policy(text):
    handle = tempfile.NamedTemporaryFile("w", suffix=".md", delete=False)
    handle.write(text)
    handle.close()
    return Path(handle.name)


def test_the_tolerance_comes_from_the_policy_file_not_a_copy_in_the_gate():
    """Plan §2: the gate CITES the constant. Change the file, change the gate.

    If this ever passes with a hardcoded 5, the policy has stopped governing
    the gate that claims to enforce it.
    """
    path = temp_policy(policy_text(7, [("WebSuite", 7)]))
    try:
        assert load_aa_tolerance(path) == 7
    finally:
        path.unlink()


def test_the_real_policy_pins_five():
    assert load_aa_tolerance() == 5


def test_a_gating_section_that_disagrees_is_refused_not_averaged():
    path = temp_policy(policy_text(5, [("WebSuite", 6)]))
    try:
        load_aa_tolerance(path)
    except PolicyError:
        return
    finally:
        path.unlink()
    raise AssertionError("two gating tolerances were reconciled instead of refused")


def test_a_section_declaring_itself_non_gating_may_differ():
    """The real file states 10 under 'Live Sites (non-gating)'.

    That suite is not in the 26 and never gates, so it is allowed its own
    number — but only because its heading says so.
    """
    path = temp_policy(policy_text(5, [("Live Sites (non-gating)", 10)]))
    try:
        assert load_aa_tolerance(path) == 5
    finally:
        path.unlink()


def test_a_non_gating_section_that_loses_its_marker_is_refused():
    """The exemption is the heading, not the section's position in the file."""
    path = temp_policy(policy_text(5, [("Live Sites", 10)]))
    try:
        load_aa_tolerance(path)
    except PolicyError:
        return
    finally:
        path.unlink()
    raise AssertionError("a gating section stating 10 was accepted")


def test_a_policy_with_no_default_section_is_refused():
    path = temp_policy("# Visual Diff Policy\n\n### WebSuite\n\n"
                       '```json\n{ "aa_tolerance": 5 }\n```\n')
    try:
        load_aa_tolerance(path)
    except PolicyError:
        return
    finally:
        path.unlink()
    raise AssertionError("a policy with no default block produced a tolerance")


def test_a_missing_policy_file_is_refused_not_defaulted():
    try:
        load_aa_tolerance(Path("/nonexistent/VISUAL_DIFF_POLICY.md"))
    except PolicyError:
        return
    raise AssertionError("a missing policy file fell back to a hardcoded default")


# ---------------------------------------------------------------------------
# The percentage half
# ---------------------------------------------------------------------------


def test_a_delta_exactly_at_tolerance_is_within_and_one_more_is_not():
    base = solid(1, 1, (100, 100, 100))
    at = solid(1, 1, (100 + TOLERANCE, 100, 100))
    over = solid(1, 1, (100 + TOLERANCE + 1, 100, 100))
    assert count_outside_tolerance(base, at, TOLERANCE) == 0
    assert count_outside_tolerance(base, over, TOLERANCE) == 1


def test_every_channel_is_compared_not_just_the_first():
    """A change confined to ONE channel must fail, whichever channel it is.

    The weaker version of this test compared red-vs-blue, which a gate that
    only ever looked at the red channel still passes. A defect that lives
    entirely in blue — a wrong link colour, a tinted gradient stop — is exactly
    the thing that would then score green forever.
    """
    base = solid(1, 1, (0, 0, 0))
    for channel in range(3):
        color = [0, 0, 0]
        color[channel] = TOLERANCE + 1
        assert count_outside_tolerance(base, solid(1, 1, tuple(color)), TOLERANCE) == 1, (
            f"a delta confined to channel {channel} was not counted"
        )

    # And an average must not launder it: two channels equal, one far off.
    assert count_outside_tolerance(base, solid(1, 1, (255, 0, 0)), TOLERANCE) == 1


def test_exactly_at_the_bar_passes_and_just_under_fails():
    chrome = solid(10, 10, (0, 0, 0))
    bad = [(i % 10, i // 10) for i in range(100)]

    at_bar = with_pixels(chrome, bad[:1], (255, 255, 255))  # 99% within
    under = with_pixels(chrome, bad[:2], (255, 255, 255))  # 98% within

    assert score("c", chrome, at_bar, [], {})["green"] is True
    result = score("c", chrome, under, [], {})
    assert result["green"] is False
    assert any(f["kind"] == "paint_below_bar" for f in result["failures"])
    assert PAINT_PASS_FRACTION == 0.99


def test_a_size_mismatch_fails_rather_than_being_scaled_to_fit():
    result = score("c", solid(4, 4, (0, 0, 0)), solid(8, 8, (0, 0, 0)), [], {})
    assert result["green"] is False
    assert result["failures"][0]["kind"] == "size_mismatch"
    assert result["discrete_failures"] == 1


# ---------------------------------------------------------------------------
# The discrete half, injected into real captures
# ---------------------------------------------------------------------------


def find_flat_element(chrome, elements, smallest=True):
    found = []
    for element in elements:
        box = px_box(element.get("rect") or {}, chrome.width, chrome.height, inset=2)
        if box and flat_color(chrome, box, TOLERANCE) is not None:
            area = (box[2] - box[0]) * (box[3] - box[1])
            found.append((area, element["selector"], box))
    found.sort(reverse=not smallest)
    return found[0] if found else None


def shift(color, amount):
    """Move every channel by `amount`, away from whichever end it is near."""
    return tuple(v - amount if v > 127 else v + amount for v in color)


def test_a_wrong_solid_color_fails_a_case_the_percentage_would_pass():
    """#83's class: a form control painted the wrong flat colour.

    `#cb1` is 81px of a 960000px viewport — 0.008%. The percentage half scores
    99.99% and passes. The case is still RED, which is the entire reason the
    discrete half exists.
    """
    chrome, elements, styles = load_case("form-controls")
    area, selector, box = find_flat_element(chrome, elements)
    want = chrome.pixel(box[0], box[1])
    rustkit = with_pixels(
        chrome,
        [(x, y) for y in range(box[1], box[3]) for x in range(box[0], box[2])],
        shift(want, TOLERANCE + 1),
    )

    result = score("form-controls", chrome, rustkit, elements, styles)
    assert result["within_fraction"] > PAINT_PASS_FRACTION, "percentage must pass"
    assert result["green"] is False, "the case must still be red"
    hits = [f for f in result["failures"] if f["kind"] == "wrong_solid_color"]
    assert len(hits) == 1, [f["kind"] for f in result["failures"]]
    assert hits[0]["selector"] == selector
    assert hits[0]["discrete"] is True


def test_a_delta_at_tolerance_is_not_a_wrong_solid_color():
    """The detector uses the pinned tolerance, not a stricter private one."""
    chrome, elements, styles = load_case("form-controls")
    area, selector, box = find_flat_element(chrome, elements)
    want = chrome.pixel(box[0], box[1])
    rustkit = with_pixels(
        chrome,
        [(x, y) for y in range(box[1], box[3]) for x in range(box[0], box[2])],
        shift(want, TOLERANCE),
    )
    result = score("form-controls", chrome, rustkit, elements, styles)
    assert not [f for f in result["failures"] if f["kind"] == "wrong_solid_color"]


def test_a_non_flat_interior_is_not_reported_as_a_wrong_solid_color():
    """A gradient where Chrome has a flat fill is a real difference — but it is
    not a WRONG SOLID COLOUR, and naming it one puts a diagnosis on the receipt
    the evidence does not support. The percentage half owns it instead.
    """
    chrome, elements, styles = load_case("form-controls")
    area, selector, box = find_flat_element(chrome, elements)
    ramp = [
        ((x, y), (x * 7 % 256, y * 11 % 256, (x + y) * 13 % 256))
        for y in range(box[1], box[3])
        for x in range(box[0], box[2])
    ]
    rustkit = chrome
    for pixel, color in ramp:
        rustkit = with_pixels(rustkit, [pixel], color)

    result = score("form-controls", chrome, rustkit, elements, styles)
    assert not [f for f in result["failures"] if f["kind"] == "wrong_solid_color"]


def first_testable_corner(chrome, elements, styles):
    """A corner whose notch backdrop is distinguishable from the fill."""
    for element in elements:
        style = styles.get(element.get("selector") or "")
        if not style:
            continue
        radius = parse_radius(style.get("border-radius"))
        if radius <= 0:
            continue
        for corner in corners(element["rect"], radius, chrome.width, chrome.height):
            if len(corner.notch) < MIN_NOTCH_PX or len(corner.inside) < MIN_NOTCH_PX:
                continue
            fill = flat_color(chrome, px_box(element["rect"], chrome.width,
                                             chrome.height, inset=2), TOLERANCE)
            fill = fill or chrome.pixel(*corner.inside[0])
            if any(
                max(abs(a - b) for a, b in zip(fill, chrome.pixel(x, y))) <= TOLERANCE
                for x, y in corner.notch
            ):
                continue
            return element["selector"], corner, fill
    return None


def test_an_unclipped_rounded_corner_fails_a_case_the_percentage_would_pass():
    """P1's corner-notch defect: the fill painted where the arc cut it away.

    36 notch pixels of a 1024000px viewport. The percentage half scores
    99.996% and passes; the case is RED anyway.
    """
    chrome, elements, styles = load_case("card-grid")
    selector, corner, fill = first_testable_corner(chrome, elements, styles)
    rustkit = with_pixels(chrome, corner.notch, fill)

    result = score("card-grid", chrome, rustkit, elements, styles)
    assert result["within_fraction"] > PAINT_PASS_FRACTION, "percentage must pass"
    assert result["green"] is False
    hits = [f for f in result["failures"] if f["kind"] == "missing_clip"]
    assert hits, [f["kind"] for f in result["failures"]]
    assert any(h["selector"] == selector for h in hits)


def test_a_correctly_clipped_corner_is_not_reported():
    chrome, elements, styles = load_case("card-grid")
    result = score("card-grid", chrome, chrome, elements, styles)
    assert result["green"] is True
    assert result["discrete_failures"] == 0


def test_a_partially_filled_notch_is_not_reported_as_a_missing_clip():
    """One stray pixel is antialiasing; the whole notch is a square corner.

    The detector demands the ENTIRE notch, so it under-fires rather than
    over-fires. For an auto-fail that is the safe direction.
    """
    chrome, elements, styles = load_case("card-grid")
    selector, corner, fill = first_testable_corner(chrome, elements, styles)
    rustkit = with_pixels(chrome, corner.notch[:-1], fill)
    result = score("card-grid", chrome, rustkit, elements, styles)
    assert not [f for f in result["failures"] if f["kind"] == "missing_clip"]


def test_a_notch_whose_backdrop_matches_the_fill_is_not_reported():
    """When Chrome shows the same colour behind the corner, there is no evidence.

    A square corner and a round one are indistinguishable against a backdrop of
    the element's own fill. Firing here would auto-fail a correct render on a
    coincidence of colour.
    """
    chrome, elements, styles = load_case("card-grid")
    selector, corner, fill = first_testable_corner(chrome, elements, styles)
    # Both sides show the fill in the notch: identical frames, no defect.
    painted = with_pixels(chrome, corner.notch, fill)
    result = score("card-grid", painted, painted, elements, styles)
    assert not [f for f in result["failures"] if f["kind"] == "missing_clip"]


def test_a_radius_the_notch_geometry_cannot_model_is_skipped_not_guessed():
    """Elliptical, percentage and per-corner radii carve a different notch.

    Applying the circular model to them would auto-fail a correct render.
    """
    assert parse_radius("8px") == 8.0
    assert parse_radius("50%") == 0.0
    assert parse_radius("8px 4px") == 0.0
    assert parse_radius("8px / 4px") == 0.0
    assert parse_radius("0px") == 0.0
    assert parse_radius(None) == 0.0


def test_chrome_against_itself_is_green_on_every_gate_case():
    """The false-positive floor.

    A detector that fires here is broken, because the two frames are the same
    picture. This is the only check that covers all 26 cases at once, and it is
    what caught the first version of the clip detector having almost no surface
    to act on.
    """
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        for case_id, case in load_case_registry().items():
            if case["scope"] == "holdout":
                continue
            image = read_png(BASELINES / case["scope"] / case_id / "baseline.png")
            write_ppm(root / case_id / "frame.ppm", image)
        report = run_gate(root)

    assert report["summary"]["measured"] == 26, report["summary"]
    assert report["summary"]["red"] == 0, [
        c["case_id"] for c in report["cases"] if not c["green"]
    ]
    assert report["summary"]["discrete_failures"] == 0
    assert gate_passes(report)


# ---------------------------------------------------------------------------
# Unmeasured is not a pass
# ---------------------------------------------------------------------------


def test_a_run_with_no_captures_fails_rather_than_reporting_all_green():
    with tempfile.TemporaryDirectory() as empty:
        report = run_gate(Path(empty))
    assert report["summary"]["measured"] == 0
    assert report["summary"]["green"] == 0
    assert not gate_passes(report)


def test_a_single_missing_capture_does_not_pass_by_omission():
    """25 perfect frames and one that never rendered is not a green run."""
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        skipped = None
        for case_id, case in sorted(load_case_registry().items()):
            if case["scope"] == "holdout":
                continue
            if skipped is None:
                skipped = case_id
                continue
            image = read_png(BASELINES / case["scope"] / case_id / "baseline.png")
            write_ppm(root / case_id / "frame.ppm", image)
        report = run_gate(root, case_ids=[skipped, "bg-solid"])

    missing = [c for c in report["cases"] if c["case_id"] == skipped][0]
    assert missing["measured"] is False
    assert missing["green"] is False
    assert missing["reason"] == "no_rustkit_capture"
    assert not gate_passes(report)


def test_only_the_registry_viewport_capture_is_scored():
    """The swarm writes several viewports per case.

    Scoring a 1920x1080 frame against an 800x600 baseline reports a page-wide
    paint catastrophe that is purely an instrument mismatch — and it looks
    exactly like the defects P1–P6 are hunting.
    """
    case_id = "bg-solid"
    case = load_case_registry()[case_id]
    native = f"{case['width']}x{case['height']}"
    chrome = read_png(BASELINES / case["scope"] / case_id / "baseline.png")

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        write_ppm(root / "run" / case_id / "1920x1080" / "iter-1" / "frame.ppm",
                  solid(1920, 1080, (255, 0, 255)))
        report = run_gate(root, case_ids=[case_id])
        assert report["cases"][0]["measured"] is False
        assert report["cases"][0]["reason"] == "no_native_viewport_capture"

        write_ppm(root / "run" / case_id / native / "iter-1" / "frame.ppm", chrome)
        report = run_gate(root, case_ids=[case_id])
        assert report["cases"][0]["measured"] is True
        assert report["cases"][0]["green"] is True


def test_an_unreadable_frame_is_unmeasured_rather_than_skipped():
    case_id = "bg-solid"
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        path = root / case_id / "frame.ppm"
        path.parent.mkdir(parents=True)
        path.write_bytes(b"P6\n800 600\n255\n" + bytes(12))
        report = run_gate(root, case_ids=[case_id])
    assert report["cases"][0]["measured"] is False
    assert report["cases"][0]["reason"].startswith("unreadable_capture")
    assert not gate_passes(report)


def test_gate_passes_refuses_any_report_that_measured_nothing():
    assert not gate_passes(
        {"summary": {"measured": 0, "red": 0, "green": 0, "total_cases": 26}}
    )
    assert gate_passes({"summary": {"measured": 1, "red": 0}})
    assert not gate_passes({"summary": {"measured": 1, "red": 1}})


def test_an_unknown_case_filter_discovers_nothing_and_fails():
    with tempfile.TemporaryDirectory() as empty:
        report = run_gate(Path(empty), case_ids=["no-such-case"])
    assert report["summary"]["total_cases"] == 0
    assert not gate_passes(report)


def test_the_holdout_scope_does_not_gate():
    """Canary-only until the 26 are green (plan §3.6)."""
    with tempfile.TemporaryDirectory() as empty:
        gating = run_gate(Path(empty))
        with_holdout = run_gate(Path(empty), include_non_gating=True)
    assert gating["summary"]["total_cases"] == 26
    assert with_holdout["summary"]["total_cases"] == 32


if __name__ == "__main__":
    assert BASELINES.exists(), f"no baselines at {BASELINES}"
    for name, fn in sorted(globals().items()):
        if name.startswith("test_") and callable(fn):
            fn()
            print(f"ok  {name}")
    print("PASS: Gate B fails structural bugs the percentage would forgive")
