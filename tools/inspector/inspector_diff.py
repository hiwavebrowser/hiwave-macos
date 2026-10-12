#!/usr/bin/env python3
"""inspector_diff.py — the first element where HiWave and Chrome diverge, and the stage.

    ours    <-- parity-capture --dump-diagnostics ours.json            (RustKit)
    chrome  <-- node tools/inspector/chrome_records.mjs <page> chrome.json  (Chromium)

Both files are element-diagnostics dumps (schema hiwave-element-diagnostics/1,
crates/rustkit-engine/src/diagnostics.rs). Elements are paired with
element_diff's keys (id, structural path, class-free path; a key duplicated on
either side is reported as ambiguous, never first-matched). Then each Chrome
element, in document order, is classified by the FIRST stage at which its
record differs:

  DOM       missing or extra element, different tag, child count, or text
            length beyond --text-tolerance          -> parsing, scripts, components
  STYLE     a comparable computed property differs  -> sheets, selectors, cascade
  GEOMETRY  a box differs by more than --threshold  -> layout and sizing
  (none)    the records agree as far as both know   -> look at paint

A divergence ROOT is a divergent element whose parent does not diverge at an
earlier stage (a child of a STYLE-divergent parent that only differs in
GEOMETRY is a consequence, not a root). See README.md for the JSON contract.

Usage:
    python3 tools/inspector/inspector_diff.py OURS.json CHROME.json \\
        [--out diff.json] [--threshold 2] [--text-tolerance 2] [--include-head] \\
        [--frame ours.ppm --overlay overlay.png]
"""

import argparse
import json
import math
import re
import sys
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "element_diff"))

import element_diff as ed  # noqa: E402

SCHEMA = "hiwave-inspector-diff/1"
DEFAULT_THRESHOLD_PX = 2.0
DEFAULT_TEXT_TOLERANCE = 2
# Plus this share of Chrome's length: whitespace placement between parsers
# moves a few characters on a long ancestor without any element differing.
TEXT_TOLERANCE_SHARE = 0.01
TOP_ROOTS = 20
STAGES = ("DOM", "STYLE", "GEOMETRY")
STAGE_RANK = {s: i for i, s in enumerate(STAGES)}

# ---------------------------------------------------------------------------
# Matching keys
# ---------------------------------------------------------------------------


def normalize_key(key: str) -> str:
    """One spelling of an oracle key for both sides.

    The engine mirrors the getSelector that captured the committed baselines
    (all classes, space-joined: ``div.card featured``); today's getSelector
    joins the first two with dots (``div.card.featured``). Both become the
    first two classes dot-joined.
    """
    if key.startswith("#"):
        return key
    out = []
    for segment in key.split(" > "):
        tag, classes, nth = ed.parse_segment(segment)
        seg = tag
        if classes:
            names = [c for c in re.split(r"[ .]+", classes) if c][:2]
            if names:
                seg += "." + ".".join(names)
        if nth:
            seg += f":nth-of-type({nth})"
        out.append(seg)
    return " > ".join(out)


def entries(dump: Dict[str, Any]) -> List[Dict[str, Any]]:
    """Records as element_diff match entries, in document order."""
    out = []
    for i, rec in enumerate(dump.get("elements") or []):
        ident = rec["identity"]
        out.append(
            {
                "selector": normalize_key(ident.get("oracle_key") or ident["selector"]),
                "id": ident.get("id"),
                "tag": ident["tag"],
                "order": i,
                "record": rec,
            }
        )
    return out


# ---------------------------------------------------------------------------
# Style comparison
# ---------------------------------------------------------------------------

# Resolved from layout (used values): a difference here is a layout result,
# compared as GEOMETRY through the boxes, not as a cascade difference.
USED_VALUE_PROPS = {
    "width", "height", "margin-top", "margin-right", "margin-bottom", "margin-left",
    "padding-top", "padding-right", "padding-bottom", "padding-left",
    "top", "right", "bottom", "left", "inset",
}
CONTAINER_FLEX = {"flex-direction", "flex-wrap", "justify-content", "align-items", "gap", "row-gap", "column-gap"}
CONTAINER_GRID = {"grid-template-columns", "grid-template-rows", "grid-auto-flow", "grid-auto-rows",
                  "grid-auto-columns", "justify-content", "align-items", "gap", "row-gap", "column-gap"}
ITEM_FLEX = {"flex-grow", "flex-shrink", "flex-basis", "align-self", "order"}
ITEM_GRID = {"grid-column-start", "grid-column-end", "grid-row-start", "grid-row-end",
             "align-self", "justify-self", "order"}
POSITIONED = {"z-index"}

# Keyword spellings that mean the same used behaviour. Applied to both sides.
EQUIVALENT = {
    "justify-content": {"normal": "flex-start", "start": "flex-start"},
    "align-items": {"normal": "stretch"},
    "align-self": {"normal": "auto"},
    "justify-self": {"normal": "auto"},
    "text-align": {"start": "left", "end": "right", "-webkit-left": "left"},
    "font-weight": {"normal": "400", "bold": "700"},
    "row-gap": {"normal": "0px"},
    "column-gap": {"normal": "0px"},
    "gap": {"normal": "0px"},
    "min-width": {"auto": "0px"},
    "min-height": {"auto": "0px"},
    "display": {"list-item": "block"},
    "background-color": {"transparent": "rgba(0, 0, 0, 0)"},
}

COLOR_RE = re.compile(r"rgba?\(\s*([\d.]+)[,\s]+([\d.]+)[,\s]+([\d.]+)(?:[,\s/]+([\d.]+%?))?\s*\)")
NUM_RE = re.compile(r"-?\d+(?:\.\d+)?(?:e-?\d+)?")


def _num(s: str) -> str:
    v = float(s)
    r = round(v, 2) + 0.0
    return str(int(r)) if r == int(r) else f"{r:g}"


def normalize_value(prop: str, value: Optional[str]) -> Optional[str]:
    """Canonical text for comparing one property's value."""
    if value is None:
        return None
    v = " ".join(str(value).strip().split())
    v = EQUIVALENT.get(prop, {}).get(v, v)
    if prop == "font-family":
        return ", ".join(f.strip().strip("'\"").lower() for f in v.split(","))

    def color(m):
        a = m.group(4)
        alpha = 1.0 if a is None else (float(a[:-1]) / 100 if a.endswith("%") else float(a))
        return f"rgba({_num(m.group(1))}, {_num(m.group(2))}, {_num(m.group(3))}, {_num(alpha)})"

    v = COLOR_RE.sub(color, v)
    if prop == "aspect-ratio" and "/" in v:
        a, b = v.replace("auto", "").split("/")[:2]
        try:
            return _num(float(a) / float(b))
        except (ValueError, ZeroDivisionError):
            return v
    v = re.sub(r"(?<![\w.-])0(?![\w.%])", "0px", v)  # bare 0 -> 0px
    return NUM_RE.sub(lambda m: _num(m.group(0)), v)


def comparable_props(rec: Dict[str, Any], parent_rec: Optional[Dict[str, Any]]) -> List[str]:
    """The properties that can matter for this element's display type."""
    style = rec.get("style") or {}
    display = style.get("display") or ""
    pdisplay = ((parent_rec or {}).get("style") or {}).get("display") or ""
    position = style.get("position") or "static"
    flex = display in ("flex", "inline-flex")
    grid = display in ("grid", "inline-grid")
    pflex = pdisplay in ("flex", "inline-flex")
    pgrid = pdisplay in ("grid", "inline-grid")
    out = []
    for p in style:
        if p in USED_VALUE_PROPS:
            continue
        if p in CONTAINER_FLEX | CONTAINER_GRID and not (
            (flex and p in CONTAINER_FLEX) or (grid and p in CONTAINER_GRID)
        ):
            continue
        if p in ITEM_FLEX | ITEM_GRID and not ((pflex and p in ITEM_FLEX) or (pgrid and p in ITEM_GRID)):
            continue
        if p in POSITIONED and position == "static" and not (pflex or pgrid):
            continue
        if display == "none" and p != "display":
            continue
        if display == "inline" and p in ("min-width", "min-height", "max-width", "max-height",
                                          "aspect-ratio", "box-sizing", "overflow-x", "overflow-y"):
            continue
        out.append(p)
    return out


def _unresolved_track_list(v: str) -> bool:
    """The engine reports grid-template-* as specified; only px lists compare."""
    return bool(re.search(r"fr|auto|minmax|repeat|%|content", v))


def style_diffs(ours: Dict[str, Any], chrome: Dict[str, Any], chrome_parent: Optional[Dict[str, Any]]) -> List[Dict[str, Any]]:
    out = []
    os_, cs = ours.get("style") or {}, chrome.get("style") or {}
    for p in comparable_props(chrome, chrome_parent):
        o, c = normalize_value(p, os_.get(p)), normalize_value(p, cs.get(p))
        if o is None or c is None or o == c:
            continue  # unknown on one side is not a difference
        if p.startswith("grid-template") and (_unresolved_track_list(o) or _unresolved_track_list(c)):
            continue
        if p == "transform" and o != "none" and c != "none":
            on, cn = [float(x) for x in NUM_RE.findall(o)], [float(x) for x in NUM_RE.findall(c)]
            if len(on) == len(cn) and all(abs(a - b) <= 0.01 for a, b in zip(on, cn)):
                continue
        out.append({"property": p, "ours": os_.get(p), "chrome": cs.get(p)})
    return out


# ---------------------------------------------------------------------------
# Classification
# ---------------------------------------------------------------------------


def _rect(r: Optional[Dict[str, float]]) -> Optional[Dict[str, float]]:
    return None if r is None else {k: float(r[k]) for k in ("x", "y", "w", "h")}


def geometry_diffs(ours: Dict[str, Any], chrome: Dict[str, Any], threshold: float) -> List[Dict[str, Any]]:
    ob, cb = ours.get("box") or {}, chrome.get("box") or {}
    if bool(ob.get("has_box")) != bool(cb.get("has_box")):
        return [{"field": "has_box", "ours": ob.get("has_box"), "chrome": cb.get("has_box"),
                 "ours_reason": ob.get("no_box_reason"), "chrome_reason": cb.get("no_box_reason")}]
    out = []
    for field in ("border_box", "content_box"):
        o, c = _rect(ob.get(field)), _rect(cb.get(field))
        if o is None or c is None:
            continue
        d = {k: round(o[k] - c[k], 2) + 0.0 for k in ("x", "y", "w", "h")}
        if max(abs(v) for v in d.values()) > threshold:
            out.append({"field": field, "ours": o, "chrome": c,
                        "dx": d["x"], "dy": d["y"], "dw": d["w"], "dh": d["h"]})
        if out:
            break  # the content box is only consulted when the border box agrees
    return out


def dom_diffs(ours: Dict[str, Any], chrome: Dict[str, Any], text_tolerance: int) -> List[Dict[str, Any]]:
    out = []
    if ours["identity"]["tag"] != chrome["identity"]["tag"]:
        out.append({"field": "tag", "ours": ours["identity"]["tag"], "chrome": chrome["identity"]["tag"]})
    ot, ct = ours.get("tree") or {}, chrome.get("tree") or {}
    if ot.get("child_count") != ct.get("child_count"):
        out.append({"field": "child_count", "ours": ot.get("child_count"), "chrome": ct.get("child_count")})
    ol, cl = ot.get("text_length"), ct.get("text_length")
    if ol is not None and cl is not None and abs(ol - cl) > max(text_tolerance, TEXT_TOLERANCE_SHARE * cl):
        out.append({"field": "text_length", "ours": ol, "chrome": cl})
    return out


def _area(r: Optional[Dict[str, float]]) -> float:
    return 0.0 if not r else max(r["w"], 0.0) * max(r["h"], 0.0)


def context_class(fc: Optional[str]) -> str:
    """`flex item of X` -> `flex item`; `absolute relative to X` -> `abspos`."""
    if not fc:
        return "no box"
    if fc.startswith("absolute"):
        return "abspos"
    return fc.split(" of ")[0]


def in_head(rec: Dict[str, Any]) -> bool:
    sel = rec["identity"]["selector"]
    return sel.startswith("html > head")


def compare(ours_doc: Dict[str, Any], chrome_doc: Dict[str, Any],
            threshold: float = DEFAULT_THRESHOLD_PX,
            text_tolerance: int = DEFAULT_TEXT_TOLERANCE,
            include_head: bool = False) -> Dict[str, Any]:
    ours, chrome = entries(ours_doc), entries(chrome_doc)
    if not include_head:
        # <head> renders nothing; its elements are matched but not classified.
        skip = lambda e: in_head(e["record"])  # noqa: E731
    else:
        skip = lambda e: False  # noqa: E731
    pairs, chrome_only, ours_only, ambiguous = ed.match(ours, chrome)
    partner = {c["order"]: (key, o) for key, o, c in pairs}
    ours_paired = {o["order"] for _, o, _ in pairs}
    ambiguous_keys = {a["key"] for a in ambiguous}

    # Tree parents from each side's own unique structural selectors.
    def parents(es):
        by_sel = {e["record"]["identity"]["selector"]: e for e in es}
        out = {}
        for e in es:
            sel = e["record"]["identity"]["selector"]
            out[e["order"]] = by_sel.get(sel.rsplit(" > ", 1)[0]) if " > " in sel else None
        return out

    chrome_parent = parents(chrome)
    ours_parent = parents(ours)
    stage_of: Dict[int, Optional[str]] = {}
    divergent: List[Dict[str, Any]] = []

    for c in chrome:
        rec = c["record"]
        par = chrome_parent[c["order"]]
        if skip(c):
            stage_of[c["order"]] = None
            continue
        found: Optional[Tuple[str, List[Dict[str, Any]]]] = None
        match_key = None
        if c["order"] in partner:
            match_key, o = partner[c["order"]]
            orec = o["record"]
            for stage, fn in (
                ("DOM", lambda: dom_diffs(orec, rec, text_tolerance)),
                ("STYLE", lambda: style_diffs(orec, rec, par["record"] if par else None)),
                ("GEOMETRY", lambda: geometry_diffs(orec, rec, threshold)),
            ):
                d = fn()
                if d:
                    found = (stage, d)
                    break
        elif c["selector"] in ambiguous_keys:
            stage_of[c["order"]] = None  # unpaired by design: not a divergence
            continue
        else:
            found = ("DOM", [{"field": "element", "ours": "missing", "chrome": "present"}])
        stage_of[c["order"]] = found[0] if found else None
        if not found:
            continue
        pstage = stage_of.get(par["order"]) if par else None
        is_root = pstage is None or STAGE_RANK[pstage] >= STAGE_RANK[found[0]]
        cbox = _rect((rec.get("box") or {}).get("border_box"))
        obox = _rect(((partner.get(c["order"]) or (None, {"record": {}}))[1]["record"].get("box") or {}).get("border_box"))
        if found[0] == "GEOMETRY" and cbox and obox:
            area = ed.mismatch_area(obox, cbox)
        else:
            area = max(_area(cbox), _area(obox))
        divergent.append({
            "order": c["order"],
            "stage": found[0],
            "root": is_root,
            "selector": rec["identity"]["selector"],
            "oracle_key": rec["identity"]["oracle_key"],
            "tag": rec["identity"]["tag"],
            "match_key": match_key,
            "formatting_context": (rec.get("box") or {}).get("formatting_context"),
            "affected_area": round(area, 2) + 0.0,
            "fields": found[1],
            "_parent_order": par["order"] if par else None,
        })

    # Extra elements: ours only. A root unless their parent's partner diverges earlier.
    chrome_order_of_ours = {o["order"]: c["order"] for _, o, c in pairs}
    for o in ours_only:
        if o["selector"] in ambiguous_keys or skip(o):
            continue
        rec = o["record"]
        par = ours_parent[o["order"]]
        pc = chrome_order_of_ours.get(par["order"]) if par else None
        obox = _rect((rec.get("box") or {}).get("border_box"))
        divergent.append({
            "order": None,
            "stage": "DOM",
            "root": True,  # DOM is the earliest stage: an extra element is always a root
            "selector": rec["identity"]["selector"],
            "oracle_key": rec["identity"]["oracle_key"],
            "tag": rec["identity"]["tag"],
            "match_key": None,
            "formatting_context": (rec.get("box") or {}).get("formatting_context"),
            "affected_area": round(_area(obox), 2) + 0.0,
            "fields": [{"field": "element", "ours": "present", "chrome": "missing"}],
            "_parent_order": pc,
        })

    roots = [d for d in divergent if d["root"]]
    in_doc = [d for d in roots if d["order"] is not None]
    first = in_doc[0] if in_doc else (roots[0] if roots else None)
    # First root with no root of the same stage below it: where the error enters.

    def has_root_below(d):
        for other in in_doc:
            p = other["_parent_order"]
            while p is not None:
                if p == d["order"] and other["stage"] == d["stage"]:
                    return True
                pe = chrome_parent.get(p)
                p = pe["order"] if pe else None
        return False

    innermost = next((d for d in in_doc if not has_root_below(d)), None)

    def public(d):
        return None if d is None else {k: v for k, v in d.items() if not k.startswith("_")}

    ranked = sorted(roots, key=lambda d: (-d["affected_area"], d["order"] if d["order"] is not None else math.inf))
    stage_counts = {s: sum(1 for d in roots if d["stage"] == s) for s in STAGES}
    style_props: Dict[str, int] = {}
    for d in roots:
        if d["stage"] == "STYLE":
            for f in d["fields"]:
                style_props[f["property"]] = style_props.get(f["property"], 0) + 1
    geometry_contexts: Dict[str, int] = {}
    for d in roots:
        if d["stage"] == "GEOMETRY":
            k = context_class(d["formatting_context"])
            geometry_contexts[k] = geometry_contexts.get(k, 0) + 1

    return {
        "schema": SCHEMA,
        "page": {
            "ours_url": (ours_doc.get("page") or {}).get("url"),
            "chrome_url": (chrome_doc.get("page") or {}).get("url"),
            "ours_engine": ((ours_doc.get("page") or {}).get("engine") or {}).get("version"),
            "chrome_engine": ((chrome_doc.get("page") or {}).get("engine") or {}).get("version"),
            "viewport": (chrome_doc.get("page") or {}).get("viewport"),
            "text_metrics_font_derived": (ours_doc.get("page") or {}).get("text_metrics_font_derived"),
        },
        "settings": {"threshold_px": threshold, "text_tolerance": text_tolerance, "include_head": include_head},
        "matching": {
            "chrome_elements": len(chrome),
            "ours_elements": len(ours),
            "matched": len(pairs),
            "chrome_only": len(chrome_only),
            "ours_only": len(ours_only),
            "ambiguous": sorted(ambiguous, key=lambda a: a["key"]),
            "truncated": bool(ours_doc.get("truncated") or chrome_doc.get("truncated")),
        },
        "first": public(first),
        "first_innermost": public(innermost),
        "roots": [public(d) for d in ranked[:TOP_ROOTS]],
        "summary": {
            "divergent_elements": len(divergent),
            "roots": len(roots),
            "stage_counts": stage_counts,
            "style_properties": dict(sorted(style_props.items())),
            "geometry_contexts": dict(sorted(geometry_contexts.items())),
        },
    }


# ---------------------------------------------------------------------------
# Report
# ---------------------------------------------------------------------------


def describe(d: Dict[str, Any]) -> str:
    parts = []
    for f in d["fields"][:6]:
        if "property" in f:
            parts.append(f"{f['property']}: ours {f['ours']!r} chrome {f['chrome']!r}")
        elif "dx" in f:
            parts.append(f"{f['field']} dx {f['dx']:+g} dy {f['dy']:+g} dw {f['dw']:+g} dh {f['dh']:+g}")
        else:
            parts.append(f"{f['field']}: ours {f['ours']!r} chrome {f['chrome']!r}")
    more = len(d["fields"]) - 6
    return "; ".join(parts) + (f"; +{more} more" if more > 0 else "")


def format_report(r: Dict[str, Any]) -> str:
    m, s = r["matching"], r["summary"]
    lines = [
        f"elements: chrome {m['chrome_elements']}, ours {m['ours_elements']}; matched {m['matched']}, "
        f"chrome-only {m['chrome_only']}, ours-only {m['ours_only']}, ambiguous {len(m['ambiguous'])}"
        + (" (TRUNCATED input)" if m["truncated"] else ""),
        f"divergence roots: {s['roots']}  DOM {s['stage_counts']['DOM']}  STYLE {s['stage_counts']['STYLE']}  "
        f"GEOMETRY {s['stage_counts']['GEOMETRY']}",
    ]
    for label, d in (("first divergence", r["first"]), ("first innermost", r["first_innermost"])):
        if d and (label == "first divergence" or d != r["first"]):
            lines.append(f"{label}: [{d['stage']}] {d['selector']}")
            lines.append(f"    {describe(d)}")
    if not r["first"]:
        lines.append("no divergence at DOM, STYLE or GEOMETRY: look at paint")
    if s["style_properties"]:
        lines.append("STYLE roots by property: " + ", ".join(f"{k} {v}" for k, v in s["style_properties"].items()))
    if s["geometry_contexts"]:
        lines.append("GEOMETRY roots by context: " + ", ".join(f"{k} {v}" for k, v in s["geometry_contexts"].items()))
    if r["roots"]:
        lines.append(f"top {len(r['roots'])} roots by affected area:")
        for d in r["roots"]:
            lines.append(f"  {d['affected_area']:>10.0f}px²  {d['stage']:<8} {d['selector']}")
            lines.append(f"  {'':>12}  {describe(d)}")
    if r["page"].get("text_metrics_font_derived") is False:
        lines.append("WARNING: our capture's text advances come from the non-font stub (Linux build); "
                     "text-sized boxes are measured against a ruler, not a font.")
    return "\n".join(lines)


def draw_overlay(result: Dict[str, Any], out_path: str, frame: Optional[str], viewport) -> int:
    """element_diff's overlay with the divergence roots that have both boxes."""
    elements = []
    for d in result["roots"]:
        g = next((f for f in d["fields"] if "dx" in f), None)
        if not g:
            continue
        elements.append({
            "tag": d["tag"], "class": None, "id": None,
            "ours": g["ours"], "chrome": g["chrome"],
            "dx": g["dx"], "dy": g["dy"], "dw": g["dw"], "dh": g["dh"],
            "max_error": max(abs(g[k]) for k in ("dx", "dy", "dw", "dh")),
            "area_error": d["affected_area"],
        })
    fake = {"summary": {"tolerance_px": -1, "threshold_px": 0}, "elements": elements}
    return ed.draw_overlay(fake, out_path, frame, viewport, None, False)


def main(argv: Optional[List[str]] = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("ours", help="parity-capture --dump-diagnostics output")
    ap.add_argument("chrome", help="tools/inspector/chrome_records.mjs output")
    ap.add_argument("--out", help="write the JSON result here (the text report then goes to stdout)")
    ap.add_argument("--threshold", type=float, default=DEFAULT_THRESHOLD_PX,
                    help="box delta (px) above which GEOMETRY diverges (default 2)")
    ap.add_argument("--text-tolerance", type=int, default=DEFAULT_TEXT_TOLERANCE,
                    help="textContent length difference tolerated before DOM diverges (default 2)")
    ap.add_argument("--include-head", action="store_true",
                    help="classify elements inside <head> too (matched either way)")
    ap.add_argument("--frame", help="our frame (PNG/PPM) for --overlay")
    ap.add_argument("--overlay", help="draw GEOMETRY roots over the frame (needs Pillow)")
    args = ap.parse_args(argv)

    ours_doc = json.loads(Path(args.ours).read_text())
    chrome_doc = json.loads(Path(args.chrome).read_text())
    result = compare(ours_doc, chrome_doc, args.threshold, args.text_tolerance, args.include_head)
    if args.overlay:
        vp = (ours_doc.get("page") or {}).get("viewport") or [1280, 800]
        n = draw_overlay(result, args.overlay, args.frame, (int(vp[0]), int(vp[1])))
        print(f"overlay: {args.overlay} ({n} roots drawn)", file=sys.stderr)
    text = json.dumps(result, indent=2, sort_keys=True)
    if args.out:
        Path(args.out).write_text(text + "\n")
        print(format_report(result))
    else:
        print(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
