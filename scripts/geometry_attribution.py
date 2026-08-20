#!/usr/bin/env python3
"""geometry_attribution.py — non-gating attribution of Gate A's failure list.

Baseline and rules: trench/BASELINE-parity-finish-line.md. Plan:
docs/PARITY_FINISH_LINE_PLAN_2026-08-04.md §2 (the gate) and its 2026-08-12
geometry-first amendment (why the grind needs aiming at geometry roots).

Gate A scores ABSOLUTE viewport coordinates and that is right for the metric: a
box in the wrong place is wrong, wherever the error came from. But a raw failure
count is a poor aiming device, because one displaced ancestor makes every
descendant fail too. This file answers a different question — WHICH failing box
is failing at its own edge — and it answers it for the grind, never for the gate.

It cannot fail a PR. Same construction as Gate C:

    ran, published, numbers terrible   -> exit 0
    ran, published, numbers perfect    -> exit 0
    could not run / measured nothing   -> exit 1

Two independent splits, and each is a claim with a stated limit.

ROOT vs CARRIED
---------------
For each failing (box, axis), subtract the same axis's delta on the nearest
ancestor BOTH sides can see. If what remains is within Gate A's tolerance, the
box is moving exactly as much as its parent already moves: CARRIED. Otherwise it
is a ROOT — something at this box's own edge is wrong.

    flex-positioning  #check1  y  expected 207, actual 199, delta -8.0
                      its own row is -8.0 too, so the residual is 0.0: CARRIED.
                      The checkbox is laid out exactly right inside its row;
                      the 8px is two text-sized boxes above it.

**CARRIED is arithmetic, not blame.** It says the parent's delta already
accounts for this one. For x/y and for a stretched width the influence really
does run parent -> child, so the reading is usually causal. For HEIGHT it often
runs the other way — a block's height comes from its children — so a carried
height means "these two moved together", never "the parent is at fault". The
split is published per axis so that reading stays available.

TEXT-REACHABLE vs FONT-INDEPENDENT
----------------------------------
A box is text-reachable when a text measurement can move its position or size:
its subtree carries a text run, or it is a form control sized from a label,
value or placeholder, or any sibling under the same parent is a text run.

That last clause is not defensive padding, it is the one that had to be
measured. `rounded-corners` lays six `.test-box` divs in a row with nothing but
source newlines between them; each collapsed space is a real glyph with a real
advance, and on a seat with no font backend the whole row staircases by
8.0 - 4.1875 per gap. A classifier that only looked INSIDE each box called all
of them font-independent and would have aimed a night's work at inline-block
positioning. With the sibling clause the board's font-independent root count
goes from 170 to 13.

**Text-reachable is a NECESSARY condition for unreadability, not proof of it.**
A text-bearing box can still have a font-independent defect — night 13's
`fit-content` sidebars were text-bearing and their defect was a 1400px stretch.
So the font-independent count is a lower bound on what a text-less seat can
score, never an upper bound on what is real.

Usage:
    python3 scripts/geometry_attribution.py --layout-root parity-baseline/captures
    python3 scripts/geometry_attribution.py --layout-root <dir> --case rounded-corners
    python3 scripts/geometry_attribution.py --layout-root <dir> --json out.json
"""

import argparse
import json
import sys
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

sys.path.insert(0, str(Path(__file__).resolve().parent))

# The tolerance, the join, the capture discovery and Chrome's own omissions all
# come from Gate A. Restating any of them here would create a second number that
# must agree with the first and eventually will not — the same reasoning that
# put the paint gate's geometry precondition on an import (2026-08-11).
from layout_oracle_gate import (  # noqa: E402
    AXES,
    GEOMETRY_TOLERANCE_PX,
    NON_GATING_SCOPES,
    border_box,
    chrome_rects_path,
    load_case_registry,
    load_json,
    find_layout_json,
)

TEXT_SIZED_CONTROLS = ("Button", "TextInput", "TextArea", "Select")


def annotate(root: Dict[str, Any]) -> Tuple[
    Dict[str, Dict[str, Any]], Dict[str, Optional[str]], Dict[str, bool]
]:
    """Walk the RustKit tree once for the three things this file needs.

    Returns (boxes, parent, text_reachable) keyed by selector:
      * boxes         selector -> the box dict
      * parent        selector -> nearest ANCESTOR selector, or None
      * text_reachable selector -> whether a text measurement can move it

    Anonymous and text boxes carry no selector and are not keys; they are still
    walked, because a text child is exactly what taints its element parent and
    its element siblings.
    """
    boxes: Dict[str, Dict[str, Any]] = {}
    parent: Dict[str, Optional[str]] = {}
    text_reachable: Dict[str, bool] = {}

    def is_text_run(node: Dict[str, Any]) -> bool:
        # ANY text node, whitespace included. A collapsed space between two
        # inline-blocks has a font-dependent advance and moves the second one.
        return node.get("type") == "text" and node.get("text") is not None

    def walk(node: Dict[str, Any], ancestor: Optional[str]) -> bool:
        selector = node.get("selector")
        if selector:
            boxes.setdefault(selector, node)
            parent.setdefault(selector, ancestor)
            ancestor = selector

        control = node.get("control_type") or ""
        own = is_text_run(node) or any(
            control.startswith(kind) for kind in TEXT_SIZED_CONTROLS
        )

        children = node.get("children") or []
        # A text run anywhere in this box's child list taints EVERY element
        # child of it, not only the ones after it: line breaking and alignment
        # move boxes in both directions.
        sibling_text = any(is_text_run(child) for child in children)
        for child in children:
            own |= walk(child, ancestor)
            child_selector = child.get("selector")
            if child_selector and sibling_text:
                text_reachable[child_selector] = True

        if selector:
            text_reachable[selector] = text_reachable.get(selector, False) or own
        return own

    walk(root, None)
    return boxes, parent, text_reachable


def nearest_comparable(
    selector: str,
    parent: Dict[str, Optional[str]],
    chrome: Dict[str, Dict[str, float]],
    rustkit: Dict[str, Dict[str, float]],
) -> Optional[str]:
    """The nearest ancestor with a rect on BOTH sides.

    Skipping past ancestors Chrome never captured is deliberate: the comparison
    needs two rects, and an ancestor present on only one side cannot supply a
    delta. Returning None means the box is compared against nothing and its
    delta is entirely its own — which is the correct reading for `html > body`.
    """
    current = parent.get(selector)
    while current is not None and (current not in chrome or current not in rustkit):
        current = parent.get(current)
    return current


def attribute_case(
    case_id: str,
    chrome_doc: Dict[str, Any],
    rustkit_doc: Dict[str, Any],
    tolerance: float = GEOMETRY_TOLERANCE_PX,
) -> Dict[str, Any]:
    """Split one case's failing axes into root/carried and reachable/independent."""
    root_node = rustkit_doc.get("root", rustkit_doc)
    boxes, parent, text_reachable = annotate(root_node)

    chrome: Dict[str, Dict[str, float]] = {}
    for element in chrome_doc.get("elements", []):
        rect = element.get("rect") or {}
        if element.get("selector") and all(axis in rect for axis in AXES):
            chrome[element["selector"]] = rect

    rustkit: Dict[str, Dict[str, float]] = {}
    for selector, box in boxes.items():
        rect = border_box(box)
        if rect is not None and all(axis in rect for axis in AXES):
            rustkit[selector] = rect

    findings: List[Dict[str, Any]] = []
    for selector in sorted(set(chrome) & set(rustkit)):
        anchor = nearest_comparable(selector, parent, chrome, rustkit)
        for axis in AXES:
            delta = float(rustkit[selector][axis]) - float(chrome[selector][axis])
            if abs(delta) <= tolerance:
                continue
            inherited = (
                float(rustkit[anchor][axis]) - float(chrome[anchor][axis])
                if anchor is not None
                else 0.0
            )
            residual = delta - inherited
            findings.append(
                {
                    "case_id": case_id,
                    "selector": selector,
                    "anchor": anchor,
                    "axis": axis,
                    "expected": float(chrome[selector][axis]),
                    "actual": float(rustkit[selector][axis]),
                    "delta": delta,
                    "residual": residual,
                    "root": abs(residual) > tolerance,
                    "text_reachable": bool(text_reachable.get(selector, False)),
                }
            )

    roots = [f for f in findings if f["root"]]
    return {
        "case_id": case_id,
        "measured": True,
        "compared": len(set(chrome) & set(rustkit)),
        "failing_axes": len(findings),
        "roots": len(roots),
        "carried": len(findings) - len(roots),
        "font_independent_roots": sum(1 for f in roots if not f["text_reachable"]),
        "findings": findings,
    }


def unmeasured_case(case_id: str, reason: str) -> Dict[str, Any]:
    """A case with no capture is UNMEASURED, never "nothing to attribute"."""
    return {
        "case_id": case_id,
        "measured": False,
        "reason": reason,
        "compared": 0,
        "failing_axes": 0,
        "roots": 0,
        "carried": 0,
        "font_independent_roots": 0,
        "findings": [],
    }


def run_attribution(
    layout_root: Path,
    case_ids: Optional[List[str]] = None,
    include_non_gating: bool = False,
    tolerance: float = GEOMETRY_TOLERANCE_PX,
) -> Dict[str, Any]:
    registry = load_case_registry()
    cases: List[Dict[str, Any]] = []

    for case_id, case in sorted(registry.items()):
        if case_ids is not None and case_id not in case_ids:
            continue
        if case["scope"] in NON_GATING_SCOPES and not include_non_gating:
            continue

        chrome_doc = load_json(chrome_rects_path(case_id, case["scope"]))
        if chrome_doc is None:
            cases.append(unmeasured_case(case_id, "no_chrome_baseline"))
            continue
        layout_path, refusal = find_layout_json(
            layout_root, case_id, f"{case['width']}x{case['height']}"
        )
        if layout_path is None:
            cases.append(unmeasured_case(case_id, refusal or "no_rustkit_capture"))
            continue
        rustkit_doc = load_json(layout_path)
        if rustkit_doc is None:
            cases.append(unmeasured_case(case_id, "unreadable_rustkit_capture"))
            continue
        cases.append(attribute_case(case_id, chrome_doc, rustkit_doc, tolerance))

    measured = [c for c in cases if c["measured"]]
    return {
        "board": "geometry-attribution",
        "gating": False,
        "tolerance_px": tolerance,
        "layout_root": str(layout_root),
        "cases": cases,
        "summary": {
            "total_cases": len(cases),
            "measured": len(measured),
            "unmeasured": len(cases) - len(measured),
            "failing_axes": sum(c["failing_axes"] for c in cases),
            "roots": sum(c["roots"] for c in cases),
            "carried": sum(c["carried"] for c in cases),
            "font_independent_roots": sum(c["font_independent_roots"] for c in cases),
        },
    }


def board_ran(report: Dict[str, Any]) -> bool:
    """Did this board measure anything?

    The ONLY thing that makes this file exit non-zero. Its numbers never do —
    "non-gating" has to mean the numbers cannot fail a PR, not "always exits 0",
    or a board that stopped being produced would look exactly like a clean one.
    """
    return report["summary"]["measured"] > 0


def print_report(report: Dict[str, Any], verbose: bool = False) -> None:
    summary = report["summary"]
    print("Geometry attribution — NON-GATING. These numbers cannot fail a PR.")
    print(f"  tolerance:  {report['tolerance_px']}px per box, per axis (Gate A's)")
    print(f"  layouts:    {report['layout_root']}")
    print(
        f"  cases:      {summary['measured']}/{summary['total_cases']} measured"
        f"  ({summary['unmeasured']} unmeasured)"
    )
    print(
        f"  failing:    {summary['failing_axes']} axes ="
        f" {summary['roots']} root + {summary['carried']} carried"
    )
    print(
        f"  of the roots, {summary['font_independent_roots']} are font-independent"
        " (a lower bound on what a seat with no text backend can score)"
    )
    print()
    print(f"  {'case':24s} {'fail':>6s} {'root':>6s} {'carried':>8s} {'font-free':>10s}")
    for case in sorted(
        report["cases"], key=lambda c: (-c["font_independent_roots"], c["case_id"])
    ):
        if not case["measured"]:
            print(f"  UNMEASURED {case['case_id']}: {case['reason']}")
            continue
        print(
            f"  {case['case_id']:24s} {case['failing_axes']:6d} {case['roots']:6d}"
            f" {case['carried']:8d} {case['font_independent_roots']:10d}"
        )

    print()
    print("  font-independent roots, worst first — the work a text-less seat can aim at:")
    aimable = [
        f
        for case in report["cases"]
        for f in case["findings"]
        if f["root"] and not f["text_reachable"]
    ]
    aimable.sort(key=lambda f: -abs(f["residual"]))
    shown = aimable if verbose else aimable[:20]
    for f in shown:
        print(
            f"    {f['case_id']} · {f['selector']} · {f['axis']} ·"
            f" {f['expected']:.4f} · {f['actual']:.4f} · {f['residual']:+.4f}"
        )
    if not aimable:
        print("    (none — every root on this board is downstream of a text measurement)")
    hidden = len(aimable) - len(shown)
    if hidden > 0:
        print(f"    … {hidden} more (use --verbose)")


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Non-gating attribution of Gate A's geometry failures"
    )
    parser.add_argument(
        "--layout-root",
        type=Path,
        required=True,
        help="Directory holding RustKit layout.json captures",
    )
    parser.add_argument(
        "--case", action="append", dest="cases", help="Limit to a case id (repeatable)"
    )
    parser.add_argument(
        "--include-non-gating",
        action="store_true",
        help="Also attribute the holdout scope",
    )
    parser.add_argument("--tolerance", type=float, default=GEOMETRY_TOLERANCE_PX)
    parser.add_argument("--json", type=Path, help="Write the full report here")
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args()

    report = run_attribution(
        args.layout_root,
        case_ids=args.cases,
        include_non_gating=args.include_non_gating,
        tolerance=args.tolerance,
    )
    print_report(report, verbose=args.verbose)

    if args.json:
        args.json.parent.mkdir(parents=True, exist_ok=True)
        with open(args.json, "w") as handle:
            json.dump(report, handle, indent=2)
        print(f"\nReport written to {args.json}")

    if not board_ran(report):
        print("\nAttribution: DID NOT RUN — it measured nothing. This is not a pass.")
        return 1
    print("\nAttribution: published (non-gating — it cannot fail this PR)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
