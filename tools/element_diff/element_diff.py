#!/usr/bin/env python3
"""element_diff.py — which element is wrong, not just that the page is.

A pixel score says a page differs from Chrome. This tool says WHERE: it joins
RustKit's layout dump to Chrome's element rects element by element and reports
each element's box on both sides, the delta, and a ranked worst list. With a
frame PNG it also draws an overlay of the mismatches.

    ours    <-- parity-capture --dump-layout layout.json   (RustKit)
    chrome  <-- baselines/<set>/<scope>/<case>/layout-rects.json  (fixtures)
                tools/parity_oracle/element_rects.mjs             (any URL)

It is a diagnostic, not a gate. Gate A (scripts/layout_oracle_gate.py) owns the
pass/fail bar; this tool reuses Gate A's rect choice and skip list so the two
never disagree about what an element's box IS, and only adds ranking, the
first-divergence pointer and the picture.

MATCHING
--------
Keys, in order of preference:

  1. ``id``    — the element's id attribute. Chrome's ``getSelector`` emits
                 ``#id`` for any element that has one, and the engine mirrors
                 that, so an id match survives any structural difference.
  2. ``path``  — the structural selector: ``tag.class:nth-of-type(n)`` chain
                 from body, exactly the string Gate A joins on.
  3. ``path_noclass`` — the same chain with classes stripped. Only tried for
                 elements still unmatched after 1 and 2, and only when the
                 stripped key is unique on BOTH sides. Catches live pages whose
                 scripts add classes on one side and not the other.

A key that occurs more than once on either side is never used to pair: the
elements are listed under ``ambiguous`` instead. First-matching a duplicate is
how a wrong pairing reads as a geometry defect.

Usage:
    python3 tools/element_diff/element_diff.py OURS.json CHROME.json \
        [--frame frame.png] [--overlay overlay.png] [--out diff.json] \
        [--tolerance 2] [--threshold 4] [--only-worst N]
"""

import argparse
import json
import math
import re
import sys
from pathlib import Path
from typing import Any, Callable, Dict, Iterator, List, Optional, Tuple

REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "scripts"))

# One definition of "the RustKit box that corresponds to getBoundingClientRect"
# and of "what Chrome's capture omits", shared with Gate A.
from layout_oracle_gate import CHROME_SKIPPED_TAGS, border_box  # noqa: E402

DEFAULT_TOLERANCE_PX = 2.0
DEFAULT_THRESHOLD_PX = 4.0
WORST_COUNT = 20

NTH_RE = re.compile(r":nth-of-type\((\d+)\)$")


# ---------------------------------------------------------------------------
# Selector parsing
# ---------------------------------------------------------------------------


def parse_segment(segment: str) -> Tuple[str, Optional[str], Optional[int]]:
    """(tag, class string or None, nth-of-type or None) of one chain segment.

    Chrome's getSelector joins up to two classes with '.' and, by a long-lived
    regex quirk, can leave a space inside one (``div.card featured``). Class
    names may contain ':' (``md:flex``), so nth-of-type is only taken from the
    END of the segment.
    """
    nth = None
    m = NTH_RE.search(segment)
    if m:
        nth = int(m.group(1))
        segment = segment[: m.start()]
    dot = segment.find(".")
    if dot < 0:
        return segment, None, nth
    return segment[:dot], segment[dot + 1 :] or None, nth


def describe_selector(selector: str) -> Dict[str, Optional[str]]:
    """tag / id / class recovered from a getSelector string."""
    if selector.startswith("#"):
        return {"tag": None, "id": selector[1:], "class": None}
    last = selector.split(" > ")[-1]
    tag, classes, _ = parse_segment(last)
    return {"tag": tag, "id": None, "class": classes.replace(".", " ") if classes else None}


def strip_classes(selector: str) -> str:
    """The structural chain with classes removed (tag:nth-of-type only)."""
    if selector.startswith("#"):
        return selector
    out = []
    for segment in selector.split(" > "):
        tag, _, nth = parse_segment(segment)
        out.append(f"{tag}:nth-of-type({nth})" if nth else tag)
    return " > ".join(out)


# ---------------------------------------------------------------------------
# Loading
# ---------------------------------------------------------------------------


def _rect(raw: Dict[str, Any]) -> Dict[str, float]:
    return {
        "x": float(raw.get("x", 0.0)),
        "y": float(raw.get("y", 0.0)),
        "w": float(raw.get("width", raw.get("w", 0.0))),
        "h": float(raw.get("height", raw.get("h", 0.0))),
    }


def walk(node: Dict[str, Any]) -> Iterator[Dict[str, Any]]:
    """Depth-first, children in order: document order for in-flow content."""
    for box, _ in walk_with_parents(node):
        yield box


def walk_with_parents(
    node: Dict[str, Any], parents: Tuple[int, ...] = ()
) -> Iterator[Tuple[Dict[str, Any], Tuple[int, ...]]]:
    """Depth-first walk yielding (box, ids of every ancestor box)."""
    yield node, parents
    for child in node.get("children") or []:
        yield from walk_with_parents(child, parents + (id(node),))


def load_ours(doc: Dict[str, Any]) -> Tuple[List[Dict[str, Any]], int]:
    """RustKit element boxes as [{selector, tag, rect, order, ancestors}],
    plus skipped count.

    ``ancestors`` is the set of ``order`` values of the kept element boxes
    above this one in the layout TREE. Ancestry is taken from the tree, never
    from selector strings: getSelector emits a bare ``#id`` for any element
    with an id, which carries no path at all.

    Boxes with no selector (anonymous, text) have no element and are not
    elements. Boxes Chrome's capture would have dropped — a skipped tag, or
    zero width AND height — are counted and dropped too, so they never show
    up as "ours only".
    """
    root = doc.get("root", doc)
    out: List[Dict[str, Any]] = []
    order_of_box: Dict[int, int] = {}
    skipped = 0
    for box, parents in walk_with_parents(root):
        selector = box.get("selector")
        if not selector:
            continue
        # An SVG shape is not a CSS box, and the engine prefixes its selector
        # with the <svg>'s own key (`#icon > rect`), a form Chrome never emits.
        # Joining on it can only produce a phantom `ours_only`.
        if box.get("type") == "svg_shape":
            skipped += 1
            continue
        rect = border_box(box)
        if rect is None:
            skipped += 1
            continue
        r = _rect(rect)
        tag = box.get("tag") or describe_selector(selector)["tag"]
        if tag in CHROME_SKIPPED_TAGS or (r["w"] == 0 and r["h"] == 0):
            skipped += 1
            continue
        ancestors = frozenset(order_of_box[p] for p in parents if p in order_of_box)
        order_of_box[id(box)] = len(out)
        out.append(
            {"selector": selector, "tag": tag, "rect": r, "order": len(out), "ancestors": ancestors}
        )
    return out, skipped


def load_chrome(doc: Dict[str, Any]) -> List[Dict[str, Any]]:
    """Chrome element rects as [{selector, tag, id, class, rect, order}].

    ``id`` / ``className`` are read when the capture carries them
    (element_rects.mjs and newer capture_baseline.mjs runs do); the committed
    baselines predate them, so they fall back to what the selector encodes.
    """
    out = []
    for i, el in enumerate(doc.get("elements") or []):
        selector = el.get("selector")
        if not selector or not isinstance(el.get("rect"), dict):
            continue
        parsed = describe_selector(selector)
        cls = el.get("className")
        out.append(
            {
                "selector": selector,
                "tag": el.get("tag") or parsed["tag"],
                "id": el.get("id") or parsed["id"],
                "class": cls if isinstance(cls, str) and cls.strip() else parsed["class"],
                "rect": _rect(el["rect"]),
                "order": i,
            }
        )
    return out


# ---------------------------------------------------------------------------
# Matching
# ---------------------------------------------------------------------------


def _id_key(entry: Dict[str, Any]) -> Optional[str]:
    sel = entry["selector"]
    if sel.startswith("#") and " > " not in sel:
        return sel[1:]
    ident = entry.get("id")
    return ident or None


KEYS = (
    ("id", _id_key),
    ("path", lambda e: e["selector"]),
    ("path_noclass", lambda e: strip_classes(e["selector"])),
)


def _index(entries: List[Dict[str, Any]], key_fn) -> Dict[str, List[Dict[str, Any]]]:
    index: Dict[str, List[Dict[str, Any]]] = {}
    for e in entries:
        k = key_fn(e)
        if k:
            index.setdefault(k, []).append(e)
    return index


def match(
    ours: List[Dict[str, Any]], chrome: List[Dict[str, Any]]
) -> Tuple[List[Tuple[str, Dict[str, Any], Dict[str, Any]]], List[Dict], List[Dict], List[Dict]]:
    """Pair elements. Returns (pairs, chrome_only, ours_only, ambiguous).

    Each pair is (match_key, ours_entry, chrome_entry). Every entry ends up in
    exactly one of: a pair, an unmatched list, or ``ambiguous``.
    """
    pairs = []
    ambiguous: List[Dict[str, Any]] = []
    left_ours = list(ours)
    left_chrome = list(chrome)
    for name, key_fn in KEYS:
        o_idx = _index(left_ours, key_fn)
        c_idx = _index(left_chrome, key_fn)
        used_o, used_c = set(), set()
        for k, cs in c_idx.items():
            os_ = o_idx.get(k)
            if not os_:
                continue
            if len(cs) > 1 or len(os_) > 1:
                # Only a FULL-selector duplicate is a real ambiguity; a
                # duplicate id or stripped path just falls through to the
                # next key.
                if name == "path":
                    ambiguous.append(
                        {"key": k, "ours_count": len(os_), "chrome_count": len(cs)}
                    )
                    used_o.update(id(o) for o in os_)
                    used_c.update(id(c) for c in cs)
                continue
            pairs.append((name, os_[0], cs[0]))
            used_o.add(id(os_[0]))
            used_c.add(id(cs[0]))
        left_ours = [o for o in left_ours if id(o) not in used_o]
        left_chrome = [c for c in left_chrome if id(c) not in used_c]
    pairs.sort(key=lambda p: p[2]["order"])
    return pairs, left_chrome, left_ours, ambiguous


# ---------------------------------------------------------------------------
# Scoring
# ---------------------------------------------------------------------------


def _area(r: Dict[str, float]) -> float:
    return max(r["w"], 0.0) * max(r["h"], 0.0)


def mismatch_area(a: Dict[str, float], b: Dict[str, float]) -> float:
    """Area covered by exactly one of the two boxes (symmetric difference).

    This is the "area-weighted error": the pixels one engine assigns to the
    element and the other does not. A 1px shift of a full-width header scores
    ~its width; a 30px shift of an icon scores ~its size. Zero iff identical.
    """
    ix = max(0.0, min(a["x"] + a["w"], b["x"] + b["w"]) - max(a["x"], b["x"]))
    iy = max(0.0, min(a["y"] + a["h"], b["y"] + b["h"]) - max(a["y"], b["y"]))
    return _area(a) + _area(b) - 2.0 * ix * iy


def _round(v: float) -> float:
    return round(v, 2) + 0.0  # +0.0 folds -0.0


def element_record(key: str, o: Dict[str, Any], c: Dict[str, Any]) -> Dict[str, Any]:
    ours, chrome = o["rect"], c["rect"]
    dx, dy = ours["x"] - chrome["x"], ours["y"] - chrome["y"]
    dw, dh = ours["w"] - chrome["w"], ours["h"] - chrome["h"]
    return {
        "path": c["selector"],
        "match_key": key,
        "tag": c["tag"],
        "id": c.get("id"),
        "class": c.get("class"),
        "ours": {k: _round(v) for k, v in ours.items()},
        "chrome": {k: _round(v) for k, v in chrome.items()},
        "dx": _round(dx),
        "dy": _round(dy),
        "dw": _round(dw),
        "dh": _round(dh),
        "max_error": _round(max(abs(dx), abs(dy), abs(dw), abs(dh))),
        "area_error": _round(mismatch_area(ours, chrome)),
    }


def first_non_ancestor(
    over: List[Dict[str, Any]], is_ancestor: Callable[[str, str], bool]
) -> Optional[Dict[str, Any]]:
    """First over-threshold element (document order) with no over-threshold
    descendant.

    The literal first divergence is almost always ``body`` or a wrapper whose
    HEIGHT is off because something inside it is; that is a symptom. The
    first element whose own subtree is clean is where the error enters.
    """
    for i, e in enumerate(over):
        if not any(is_ancestor(e["path"], d["path"]) for d in over[i + 1 :]):
            return e
    return None


def compare(
    ours_doc: Dict[str, Any],
    chrome_doc: Dict[str, Any],
    tolerance: float = DEFAULT_TOLERANCE_PX,
    threshold: float = DEFAULT_THRESHOLD_PX,
    worst: int = WORST_COUNT,
) -> Dict[str, Any]:
    ours, ours_skipped = load_ours(ours_doc)
    chrome = load_chrome(chrome_doc)
    pairs, chrome_only, ours_only, ambiguous = match(ours, chrome)

    elements = [element_record(k, o, c) for k, o, c in pairs]
    within = [e for e in elements if e["max_error"] <= tolerance]
    over = [e for e in elements if e["max_error"] > threshold]
    first = over[0] if over else None  # elements are in Chrome document order
    # Ancestry of matched elements, from our layout tree. Matched paths are
    # unique (pairs are 1:1 and Chrome's selectors are unique per page).
    ours_by_path = {c["selector"]: o for _, o, c in pairs}
    leaf = first_non_ancestor(
        over,
        lambda a, d: ours_by_path[a]["order"] in ours_by_path[d]["ancestors"],
    )
    ranked = sorted(
        (e for e in elements if e["area_error"] > 0),
        key=lambda e: (-e["area_error"], -e["max_error"]),
    )

    def brief(e):
        return {k: e[k] for k in ("path", "tag", "class", "dx", "dy", "dw", "dh", "max_error", "area_error")}

    by_key: Dict[str, int] = {}
    for k, _, _ in pairs:
        by_key[k] = by_key.get(k, 0) + 1

    summary = {
        "tolerance_px": tolerance,
        "threshold_px": threshold,
        "chrome_elements": len(chrome),
        "ours_elements": len(ours),
        "ours_skipped": ours_skipped,
        "matched": len(elements),
        "matched_by": by_key,
        "chrome_only": len(chrome_only),
        "ours_only": len(ours_only),
        "ambiguous": len(ambiguous),
        "within_tolerance": len(within),
        "within_tolerance_share": round(len(within) / len(elements), 4) if elements else None,
        "over_threshold": len(over),
        "first_over_threshold": brief(first) if first else None,
        "first_over_threshold_non_ancestor": brief(leaf) if leaf else None,
        "worst": [brief(e) for e in ranked[:worst]],
        "text_backend": ours_doc.get("text_backend"),
        "text_metrics_font_derived": ours_doc.get("text_metrics_font_derived"),
    }
    if ours_doc.get("text_metrics_font_derived") is False:
        summary["warning"] = (
            "RustKit text advances are not font-derived in this capture "
            f"(text_backend={ours_doc.get('text_backend')}); any element sized "
            "by text is measured against a stub, not a font."
        )

    def unmatched(e):
        r = {"path": e["selector"], "tag": e["tag"], "rect": {k: _round(v) for k, v in e["rect"].items()}}
        if e.get("class"):
            r["class"] = e["class"]
        return r

    return {
        "summary": summary,
        "elements": elements,
        "unmatched": {
            "chrome_only": [unmatched(e) for e in chrome_only],
            "ours_only": [unmatched(e) for e in ours_only],
        },
        "ambiguous": ambiguous,
    }


# ---------------------------------------------------------------------------
# Overlay
# ---------------------------------------------------------------------------

RED = (220, 30, 30, 255)
BLUE = (30, 80, 230, 255)
GREEN = (20, 170, 60, 160)


def label_for(e: Dict[str, Any]) -> str:
    name = e["tag"] or ""
    if e.get("class"):
        name += "." + e["class"].split()[0]
    elif e.get("id"):
        name += "#" + e["id"]
    return f"{name} {e['dx']:+g},{e['dy']:+g} {e['dw']:+g},{e['dh']:+g}"


def draw_overlay(
    result: Dict[str, Any],
    out_path: str,
    frame_path: Optional[str] = None,
    viewport: Optional[Tuple[int, int]] = None,
    only_worst: Optional[int] = None,
    draw_green: bool = True,
):
    from PIL import Image, ImageDraw, ImageFont

    if frame_path:
        base = Image.open(frame_path).convert("RGBA")
    else:
        w, h = viewport or (1280, 800)
        base = Image.new("RGBA", (w, h), (255, 255, 255, 255))
    layer = Image.new("RGBA", base.size, (0, 0, 0, 0))
    draw = ImageDraw.Draw(layer)
    font = ImageFont.load_default()

    summary = result["summary"]
    elements = result["elements"]
    if draw_green:
        for e in elements:
            if e["max_error"] <= summary["tolerance_px"]:
                draw.rectangle(_xyxy(e["ours"]), outline=GREEN, width=1)

    bad = [e for e in elements if e["max_error"] > summary["threshold_px"]]
    bad.sort(key=lambda e: (-e["area_error"], -e["max_error"]))
    if only_worst is not None:
        bad = bad[:only_worst]
    # Draw largest first so small boxes and their labels stay on top.
    for e in bad:
        draw.rectangle(_xyxy(e["chrome"]), outline=BLUE, width=2)
        draw.rectangle(_xyxy(e["ours"]), outline=RED, width=2)
    for e in reversed(bad):
        text = label_for(e)
        x, y = e["ours"]["x"], e["ours"]["y"]
        x = min(max(x, 0), base.size[0] - 8)
        y = min(max(y - 12, 0), base.size[1] - 12)
        box = draw.textbbox((x, y), text, font=font)
        draw.rectangle((box[0] - 1, box[1] - 1, box[2] + 1, box[3] + 1), fill=(255, 255, 255, 220))
        draw.text((x, y), text, fill=RED, font=font)

    Image.alpha_composite(base, layer).convert("RGB").save(out_path)
    return len(bad)


def _xyxy(r: Dict[str, float]) -> Tuple[float, float, float, float]:
    return (r["x"], r["y"], r["x"] + max(r["w"] - 1, 0), r["y"] + max(r["h"] - 1, 0))


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def format_summary(s: Dict[str, Any]) -> str:
    share = s["within_tolerance_share"]
    lines = [
        f"elements: chrome {s['chrome_elements']}, ours {s['ours_elements']} "
        f"(+{s['ours_skipped']} skipped as Chrome would)",
        f"matched {s['matched']} {s['matched_by']}; chrome-only {s['chrome_only']}, "
        f"ours-only {s['ours_only']}, ambiguous {s['ambiguous']}",
        f"within {s['tolerance_px']:g}px: {s['within_tolerance']}"
        + (f" ({share * 100:.1f}%)" if share is not None else ""),
        f"over {s['threshold_px']:g}px: {s['over_threshold']}",
    ]
    f = s["first_over_threshold"]
    if f:
        lines.append(
            f"first over {s['threshold_px']:g}px (document order): {f['path']} "
            f"dx {f['dx']:+g} dy {f['dy']:+g} dw {f['dw']:+g} dh {f['dh']:+g}"
        )
    g = s["first_over_threshold_non_ancestor"]
    if g and g is not f:
        lines.append(
            f"first over {s['threshold_px']:g}px with a clean subtree: {g['path']} "
            f"dx {g['dx']:+g} dy {g['dy']:+g} dw {g['dw']:+g} dh {g['dh']:+g}"
        )
    if s["worst"]:
        lines.append("worst by mismatched area:")
        for e in s["worst"]:
            lines.append(
                f"  {e['area_error']:>10.0f}px²  max {e['max_error']:>7.2f}  {e['path']}"
            )
    if s.get("warning"):
        lines.append("WARNING: " + s["warning"])
    return "\n".join(lines)


def main(argv: Optional[List[str]] = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("ours", help="RustKit layout dump (parity-capture --dump-layout)")
    ap.add_argument("chrome", help="Chrome rects (layout-rects.json / element_rects.mjs)")
    ap.add_argument("--frame", help="RustKit frame PNG (parity-capture --dump-frame)")
    ap.add_argument("--overlay", help="write the overlay PNG here")
    ap.add_argument("--out", help="write the full JSON result here (default: stdout)")
    ap.add_argument("--tolerance", type=float, default=DEFAULT_TOLERANCE_PX,
                    help="max |delta| counted as matching (default 2)")
    ap.add_argument("--threshold", type=float, default=DEFAULT_THRESHOLD_PX,
                    help="|delta| above which an element is drawn and ranked as wrong (default 4)")
    ap.add_argument("--only-worst", type=int, metavar="N",
                    help="overlay: draw only the N worst over-threshold elements")
    ap.add_argument("--no-green", action="store_true", help="overlay: skip within-tolerance boxes")
    args = ap.parse_args(argv)

    ours_doc = json.loads(Path(args.ours).read_text())
    chrome_doc = json.loads(Path(args.chrome).read_text())
    result = compare(ours_doc, chrome_doc, args.tolerance, args.threshold)

    if args.overlay:
        vp = ours_doc.get("viewport") or chrome_doc.get("viewport") or {}
        size = (int(vp.get("width") or 1280), int(vp.get("height") or 800))
        drawn = draw_overlay(result, args.overlay, args.frame, size, args.only_worst, not args.no_green)
        print(f"overlay: {args.overlay} ({drawn} mismatched elements drawn)", file=sys.stderr)

    if args.out:
        Path(args.out).write_text(json.dumps(result, indent=2))
        print(format_summary(result["summary"]))
    else:
        json.dump(result, sys.stdout, indent=2)
        print()
    return 0


if __name__ == "__main__":
    sys.exit(main())
