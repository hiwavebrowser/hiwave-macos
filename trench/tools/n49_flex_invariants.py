#!/usr/bin/env python3
"""Font-independent flex alignment invariants, checked on RustKit's OWN capture.

WHY THIS EXISTS
---------------
Gate A scores RustKit's absolute rects against Chrome's, which is right for the
metric and useless on this trench seat for anything text touches: there is no
font backend here (night 48 — `measure_text_advanced` answers half an em per
character), so `flex-positioning`'s 174 failing axes are 0 font-independent
roots under `scripts/geometry_attribution.py`'s strict column. P3 has had no
readable surface since 2026-08-20 for that reason.

This board asks a different question, and it never reads Chrome's rects:

    given RUSTKIT's own item sizes, does RustKit place those items the way
    justify-content and align-items say it must?

That question is font-independent by construction. An item whose width is
wrong because a glyph was measured with no font still has to sit flush against
the content edge under `flex-start`, and still has to leave equal space on both
sides under `center`. Chrome's `computed-styles.json` is used only to read what
the author asked for — never as an expected geometry.

WHAT IT CANNOT SAY
------------------
- **A clean row is not a correct row.** The invariants are necessary, not
  sufficient: an item at the right offset can still be the wrong size, and this
  board is deliberately blind to size. Gate A owns that.
- **Where an item's cross size is text-derived on this seat, only the BOOLEAN
  transfers, not the magnitude.** A 0.6px asymmetry says the centring
  arithmetic disagrees with itself; it does not predict 0.6px on macOS.
- **Skips are reported, never counted as passes** (the fleet rule banked in
  `trench/BASELINE-parity-finish-line.md`: a blank row is not a pass). Multi-line
  containers need align-content and are out of scope; reverse directions are
  unmodelled.

Diagnostic only, no mutation-checked guards — do not wire into CI. Same
standing as `trench/tools/n45_capture_all.py`.

    python3 trench/tools/n49_flex_invariants.py <capture-root> [case-id]
"""
import collections
import json
import pathlib
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "scripts"))
import layout_oracle_gate as g  # noqa: E402

# The same constant Gate A uses. Imported, never restated: two tolerances that
# must agree and are written down twice will disagree (night 8).
TOL = g.GEOMETRY_TOLERANCE_PX

PACKED = ("normal", "flex-start", "start", "left", "flex-end", "end", "right", "center")


def px(value, default=0.0):
    try:
        return float(str(value).replace("px", ""))
    except (TypeError, ValueError):
        return default


def walk(node, out):
    out.append(node)
    for child in node.get("children") or []:
        walk(child, out)


def gap_px(styles, axis_is_row):
    parts = str(styles.get("gap", "normal")).split()
    row, col = (parts[0], parts[1]) if len(parts) == 2 else (parts[0], parts[0]) if parts else ("normal", "normal")
    value = col if axis_is_row else row
    return 0.0 if value == "normal" else px(value)


def run(capture_root: pathlib.Path, only=None):
    violations = []
    skips = collections.Counter()
    measured = 0
    cases_seen = 0

    for case_id, case in sorted(g.load_case_registry().items()):
        if case.get("scope") == "holdout":
            continue
        if only and case_id != only:
            continue
        layout_path = capture_root / case_id / "layout.json"
        styles_path = g.baselines_dir() / case["scope"] / case_id / "computed-styles.json"
        if not layout_path.exists() or not styles_path.exists():
            skips["case has no capture or no computed styles"] += 1
            continue
        cases_seen += 1
        styles = {e["selector"]: e["styles"] for e in json.load(open(styles_path))["elements"]}
        boxes = []
        walk(json.load(open(layout_path))["root"], boxes)

        for box in boxes:
            selector = box.get("selector")
            if not selector or selector not in styles:
                continue
            style = styles[selector]
            if style.get("display") not in ("flex", "inline-flex"):
                continue
            if style.get("flex-wrap") != "nowrap":
                skips["multi-line container (needs align-content)"] += 1
                continue
            direction = style.get("flex-direction", "row")
            if direction.endswith("reverse"):
                skips["reverse direction (unmodelled)"] += 1
                continue

            kids = box.get("children") or []
            items = [k for k in kids if k.get("selector")]
            anon_with_area = [
                k
                for k in kids
                if not k.get("selector")
                and ((k.get("border_box") or {}).get("width", 0) or (k.get("border_box") or {}).get("height", 0))
            ]
            if not items:
                skips["no element children in the RustKit box"] += 1
                continue
            if anon_with_area:
                skips["an anonymous box with area — the item sets disagree"] += 1
                continue
            if any(k["selector"] not in styles for k in items):
                skips["a child Chrome does not report"] += 1
                continue
            items = [
                k for k in items if styles[k["selector"]].get("position") in ("static", "relative")
            ]
            if not items:
                skips["every child is out of flow"] += 1
                continue

            row = direction.startswith("row")
            content = box["content_rect"]
            if row:
                main0, main1 = content["x"], content["x"] + content["width"]
                cross0, cross1 = content["y"], content["y"] + content["height"]
            else:
                main0, main1 = content["y"], content["y"] + content["height"]
                cross0, cross1 = content["x"], content["x"] + content["width"]
            gap = gap_px(style, row)
            measured += 1

            def outer(k):
                m = k["margin_box"]
                return (m["x"], m["x"] + m["width"]) if row else (m["y"], m["y"] + m["height"])

            def outer_cross(k):
                m = k["margin_box"]
                return (m["y"], m["y"] + m["height"]) if row else (m["x"], m["x"] + m["width"])

            def check(rule, expected, actual, suffix=""):
                if abs(expected - actual) > TOL:
                    violations.append(
                        {
                            "case_id": case_id,
                            "selector": selector + suffix,
                            "rule": rule,
                            "expected": expected,
                            "actual": actual,
                            "delta": actual - expected,
                        }
                    )

            spans = [outer(k) for k in items]
            n = len(spans)
            leading = spans[0][0] - main0
            trailing = main1 - spans[-1][1]
            free = (main1 - main0) - sum(b - a for a, b in spans) - gap * (n - 1)
            jc = style.get("justify-content", "normal")

            if jc in ("normal", "flex-start", "start", "left"):
                check(f"justify:{jc} leading", 0.0, leading)
            elif jc in ("flex-end", "end", "right"):
                check(f"justify:{jc} trailing", 0.0, trailing)
            elif jc == "center":
                check("justify:center symmetry", leading, trailing)
            elif jc == "space-between" and n >= 2 and free > TOL:
                check("justify:space-between leading", 0.0, leading)
                check("justify:space-between trailing", 0.0, trailing)
            elif jc == "space-around" and free > TOL:
                check("justify:space-around leading", free / (2 * n), leading)
                check("justify:space-around trailing", free / (2 * n), trailing)
            elif jc == "space-evenly" and free > TOL:
                check("justify:space-evenly leading", free / (n + 1), leading)
                check("justify:space-evenly trailing", free / (n + 1), trailing)

            if jc in PACKED:
                for i in range(1, n):
                    check(f"item-gap[{i}]", gap, spans[i][0] - spans[i - 1][1])

            for k in items:
                kstyle = styles[k["selector"]]
                align = kstyle.get("align-self", "auto")
                if align == "auto":
                    align = style.get("align-items", "normal")
                if align == "baseline":
                    skips["baseline alignment (unmodelled)"] += 1
                    continue
                k0, k1 = outer_cross(k)
                klead, ktrail = k0 - cross0, cross1 - k1
                suffix = " > " + k["selector"].rsplit(">", 1)[-1].strip()
                if align in ("flex-start", "start", "self-start"):
                    check(f"align:{align} leading", 0.0, klead, suffix)
                elif align in ("flex-end", "end", "self-end"):
                    check(f"align:{align} trailing", 0.0, ktrail, suffix)
                elif align == "center":
                    check("align:center symmetry", klead, ktrail, suffix)
                elif align in ("normal", "stretch"):
                    cross_prop = "height" if row else "width"
                    if kstyle.get(cross_prop, "auto") == "auto":
                        check("align:stretch fills the cross axis", cross1 - cross0, k1 - k0, suffix)
                    else:
                        check(f"align:stretch(explicit {cross_prop}) leading", 0.0, klead, suffix)

    return {
        "cases": cases_seen,
        "measured": measured,
        "skipped": sum(skips.values()),
        "skip_reasons": dict(skips),
        "violations": violations,
    }


def main() -> int:
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    report = run(pathlib.Path(sys.argv[1]), sys.argv[2] if len(sys.argv) > 2 else None)
    print(
        f"{report['cases']} cases · flex containers measured {report['measured']}, "
        f"skipped {report['skipped']} · violations {len(report['violations'])}"
    )
    for reason, count in sorted(report["skip_reasons"].items(), key=lambda kv: -kv[1]):
        print(f"  skipped {count:4d}  {reason}")
    for v in sorted(report["violations"], key=lambda v: -abs(v["delta"])):
        print(
            f"{v['case_id']:22s} {v['rule']:38s} exp {v['expected']:9.4f} "
            f"act {v['actual']:9.4f} d {v['delta']:+9.4f}  {v['selector']}"
        )
    # Non-gating in the same sense as Gate C: numbers never fail anything, but a
    # board that measured nothing is a broken board, not a clean one.
    return 0 if report["measured"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
