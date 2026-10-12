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

A divergence ROOT is a divergent element that is not a consequence:
  - its parent does not diverge at an earlier stage (a child of a
    STYLE-divergent parent that only differs in GEOMETRY is a consequence);
  - a GEOMETRY difference is not explained by its neighbours' deltas: moved
    with its parent's content box or its previous sibling's far edge, a width
    that follows the parent's, a height that follows its own children;
  - a STYLE difference is not only an inherited value equal to the parent's
    on both sides.
`summary.stage_counts` counts roots only. See README.md for the JSON contract.

Usage:
    python3 tools/inspector/inspector_diff.py OURS.json CHROME.json \\
        [--out diff.json] [--threshold 2] [--text-tolerance 2] [--include-head] \\
        [--frame ours.ppm --overlay overlay.png]
"""

import argparse
import json
import math
import os
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
# Elements whose content is replaced; object-fit applies only to them.
REPLACED = {"img", "video", "canvas", "iframe", "embed", "object", "input", "svg"}

# Keyword spellings that mean the same used behaviour. Applied to both sides.
# `start`/`end` and `flex-start`/`flex-end` place items the same way in a
# flex or grid container (the engine maps the former on parse). z-index
# `auto` and `0` stack differently, but the engine stores both as 0 and
# cannot tell them apart, so a difference there would be invented.
EQUIVALENT = {
    "justify-content": {"normal": "flex-start", "start": "flex-start", "end": "flex-end"},
    "align-items": {"normal": "stretch", "start": "flex-start", "end": "flex-end"},
    "align-self": {"normal": "auto", "start": "flex-start", "end": "flex-end"},
    "justify-self": {"normal": "auto"},
    "text-align": {"start": "left", "end": "right", "-webkit-left": "left",
                   "-webkit-center": "center", "-webkit-right": "right"},
    "z-index": {"0": "auto"},
    "font-weight": {"normal": "400", "bold": "700"},
    "row-gap": {"normal": "0px"},
    "column-gap": {"normal": "0px"},
    "gap": {"normal": "0px"},
    "display": {"list-item": "block"},
    "background-color": {"transparent": "rgba(0, 0, 0, 0)"},
}

NAMED_COLORS = {"black": "rgb(0, 0, 0)", "white": "rgb(255, 255, 255)", "transparent": "rgba(0, 0, 0, 0)"}
HEX_RE = re.compile(r"#([0-9a-fA-F]{3,4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})\b")
GAP_PROPS = {"gap", "row-gap", "column-gap"}
URL_RE = re.compile(r'url\(\s*"?([^")]*)"?\s*\)')


def _hex(m: "re.Match[str]") -> str:
    h = m.group(1)
    if len(h) in (3, 4):
        h = "".join(ch * 2 for ch in h)
    r, g, b = (int(h[i:i + 2], 16) for i in (0, 2, 4))
    a = int(h[6:8], 16) / 255 if len(h) == 8 else 1.0
    return f"rgba({r}, {g}, {b}, {a})"


def _matrix(v: str) -> str:
    """`matrix3d` that is a 2D affine as `matrix`; the identity as `none`."""
    m = re.fullmatch(r"matrix3d\((.*)\)", v)
    if m:
        n = [float(x) for x in m.group(1).split(",")]
        flat = [n[2], n[3], n[6], n[7], n[8], n[9], n[11], n[14]]
        if len(n) == 16 and all(abs(x) < 1e-6 for x in flat) and abs(n[10] - 1) < 1e-6 and abs(n[15] - 1) < 1e-6:
            v = f"matrix({n[0]}, {n[1]}, {n[4]}, {n[5]}, {n[12]}, {n[13]})"
    m = re.fullmatch(r"matrix\((.*)\)", v)
    if m:
        n = [float(x) for x in m.group(1).split(",")]
        if len(n) == 6 and all(abs(a - b) < 1e-6 for a, b in zip(n, (1, 0, 0, 1, 0, 0))):
            return "none"
    return v


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
    if prop in GAP_PROPS:
        # Per side: `gap: normal 5px` is row-gap normal, column-gap 5px.
        parts = ["0px" if t == "normal" else t for t in v.split(" ")]
        v = parts[0] if len(set(parts)) == 1 else " ".join(parts)
    if prop == "transform":
        v = _matrix(v)
    if prop == "background-image":
        v = re.sub(r"-webkit-((?:repeating-)?(?:linear|radial|conic)-gradient)\([^)]*(?:\([^)]*\)[^)]*)*\)",
                   r"\1(…)", v)
    v = re.sub(r"\b(black|white|transparent)\b", lambda m: NAMED_COLORS[m.group(1)], v)
    v = HEX_RE.sub(_hex, v)

    def color(m):
        a = m.group(4)
        alpha = 1.0 if a is None else (float(a[:-1]) / 100 if a.endswith("%") else float(a))
        if alpha == 0:
            return "rgba(0, 0, 0, 0)"  # every transparent colour paints the same
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
    """The properties that can matter for this element's display type.

    float/clear do nothing on a flex or grid item (Chromium still reports
    them), vertical-align only applies to inline-level boxes and table
    cells, object-fit only to replaced elements."""
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
        if p in ("float", "clear") and (pflex or pgrid):
            continue
        if p == "vertical-align" and not (display.startswith("inline") or display == "table-cell"):
            continue
        if p == "object-fit" and rec["identity"]["tag"] not in REPLACED:
            continue
        if display == "inline" and p in ("min-width", "min-height", "max-width", "max-height",
                                          "aspect-ratio", "box-sizing", "overflow-x", "overflow-y"):
            continue
        out.append(p)
    return out


def _unresolved_track_list(v: str) -> bool:
    """Only px lists compare: anything else, or `none` (a grid container's
    implicit tracks), is a list one side has not resolved."""
    return v == "none" or bool(re.search(r"fr|auto|minmax|repeat|%|content|\[", v))


def _same_urls(o: str, c: str) -> bool:
    """background-image layers with relative URLs on one side.

    The engine keeps a URL as specified (relative to its stylesheet, which
    the record does not name); Chromium resolves it. Equal when every layer
    is equal, or Chromium's absolute URL ends with ours' relative path.
    """
    ou, cu = URL_RE.findall(o), URL_RE.findall(c)
    if len(ou) != len(cu) or URL_RE.sub("url()", o) != URL_RE.sub("url()", c):
        return False
    for a, b in zip(ou, cu):
        if a == b:
            continue
        rel, absu = (a, b) if "://" not in a else (b, a)
        if "://" in rel or rel.startswith("/") or ".." in rel:
            return False
        rel = re.sub(r"^(\./)+", "", rel)
        if not absu.endswith("/" + rel):
            return False
    return True


def style_diffs(ours: Dict[str, Any], chrome: Dict[str, Any], chrome_parent: Optional[Dict[str, Any]]) -> List[Dict[str, Any]]:
    out = []
    os_, cs = ours.get("style") or {}, chrome.get("style") or {}
    pdisplay = ((chrome_parent or {}).get("style") or {}).get("display") or ""
    item = pdisplay in ("flex", "inline-flex", "grid", "inline-grid")
    for p in comparable_props(chrome, chrome_parent):
        o, c = normalize_value(p, os_.get(p)), normalize_value(p, cs.get(p))
        if p in ("min-width", "min-height") and not item:
            # auto computes to 0 outside flex and grid items; on an item the
            # two differ and are compared as written.
            o, c = ("0px" if v == "auto" else v for v in (o, c))
        if o is None or c is None or o == c:
            continue  # unknown on one side is not a difference
        if p.startswith("grid-template") and (_unresolved_track_list(o) or _unresolved_track_list(c)):
            continue
        if p == "background-image" and _same_urls(o, c):
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


def page_text_lengths(es: List[Dict[str, Any]]) -> Dict[int, Optional[int]]:
    """textContent length without the text of <style>/<script> descendants:
    a stylesheet or script edit is not a DOM difference of its ancestors."""
    by_sel = {e["record"]["identity"]["selector"]: e["order"] for e in es}
    hidden = {e["order"]: 0 for e in es}
    for e in es:
        if e["tag"] not in ("style", "script"):
            continue
        n = (e["record"].get("tree") or {}).get("text_length") or 0
        sel = e["record"]["identity"]["selector"]
        while " > " in sel:
            sel = sel.rsplit(" > ", 1)[0]
            if sel in by_sel:
                hidden[by_sel[sel]] += n
    out = {}
    for e in es:
        n = (e["record"].get("tree") or {}).get("text_length")
        out[e["order"]] = None if n is None else n - hidden[e["order"]]
    return out


def dom_diffs(ours: Dict[str, Any], chrome: Dict[str, Any], text_tolerance: int,
              lengths: Optional[Tuple[Optional[int], Optional[int]]] = None) -> List[Dict[str, Any]]:
    out = []
    if ours["identity"]["tag"] != chrome["identity"]["tag"]:
        out.append({"field": "tag", "ours": ours["identity"]["tag"], "chrome": chrome["identity"]["tag"]})
    ot, ct = ours.get("tree") or {}, chrome.get("tree") or {}
    if ot.get("child_count") != ct.get("child_count"):
        out.append({"field": "child_count", "ours": ot.get("child_count"), "chrome": ct.get("child_count")})
    ol, cl = lengths if lengths else (ot.get("text_length"), ct.get("text_length"))
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


# Inherited properties among STYLE_PROPERTIES: a child that carries its
# parent's value on both sides only repeats the parent's difference.
INHERITED = {"color", "font-family", "font-size", "font-weight", "line-height",
             "text-align", "visibility", "white-space"}


def _delta(o: Optional[Dict[str, float]], c: Optional[Dict[str, float]]) -> Optional[Dict[str, float]]:
    if not o or not c:
        return None
    return {k: o[k] - c[k] for k in ("x", "y", "w", "h")}


class _Boxes:
    """Ours-minus-Chrome deltas of the paired elements' boxes, by Chrome order."""

    def __init__(self, partner, chrome_by_order):
        self.border, self.content, self.ours, self.chrome = {}, {}, {}, {}
        for order, (_, o) in partner.items():
            ob = o["record"].get("box") or {}
            cb = chrome_by_order[order]["record"].get("box") or {}
            if not (ob.get("has_box") and cb.get("has_box")):
                continue
            self.ours[order], self.chrome[order] = ob, cb
            self.border[order] = _delta(_rect(ob.get("border_box")), _rect(cb.get("border_box")))
            self.content[order] = _delta(_rect(ob.get("content_box")), _rect(cb.get("content_box")))


def _within(v: float, lo: float, hi: float, thr: float) -> bool:
    return lo - thr <= v <= hi + thr


def _geometry_explained(order, boxes, parent_order, prev_order, children, cb_order, threshold) -> bool:
    """A GEOMETRY difference that follows from its neighbours' differences.

    Position: each axis moved with the parent's (or abspos containing
    block's) content box, anywhere within the room that box gained or lost
    (centring, distribution), or with the far edge of the previous sibling.
    Width: unchanged, or shrank/grew with the parent's content box (top-down).
    Height: unchanged, or equal to the change in its in-flow children's
    extent with its own padding and border unchanged (an auto height,
    bottom-up). Anything else is the element's own: a root.
    """
    d = boxes.border.get(order)
    if d is None:
        return False
    own = boxes.content.get(order)
    refs = []
    for ref in (parent_order, cb_order):
        rd = boxes.content.get(ref) if ref is not None else None
        if rd:
            refs.append(((rd["x"], rd["x"] + min(0.0, rd["w"]), rd["x"] + max(0.0, rd["w"])),
                         (rd["y"], rd["y"] + min(0.0, rd["h"]), rd["y"] + max(0.0, rd["h"]))))
    pd = boxes.border.get(prev_order) if prev_order is not None else None
    if pd:
        ex, ey = pd["x"] + pd["w"], pd["y"] + pd["h"]
        refs.append(((ex, ex, ex), (ey, ey, ey)))

    def axis(v, i):
        return abs(v) <= threshold or any(_within(v, min(r[i][1], r[i][0]), max(r[i][2], r[i][0]), threshold)
                                          for r in refs)

    if not (axis(d["x"], 0) and axis(d["y"], 1)):
        return False
    pc = boxes.content.get(parent_order) if parent_order is not None else None
    if abs(d["w"]) > threshold:
        if not (pc and pc["w"] * d["w"] > 0 and abs(d["w"]) <= abs(pc["w"]) + threshold):
            return False
    if abs(d["h"]) > threshold:
        if own is None or abs(d["h"] - own["h"]) > threshold:
            return False  # its own padding or border changed
        flow = [k for k in children if k in boxes.border
                and not (boxes.chrome[k].get("formatting_context") or "").startswith(("absolute", "fixed"))]
        if not flow:
            return False

        def extent(side):
            top = side[order]["content_box"]["y"]
            return max(side[k]["border_box"]["y"] + side[k]["border_box"]["h"] for k in flow) - top

        if abs((extent(boxes.ours) - extent(boxes.chrome)) - own["h"]) > threshold:
            return False
    return True


def _inherited_only(fields, ours_parent, chrome_parent) -> List[Dict[str, Any]]:
    """The STYLE fields that are not just the parent's inherited value."""
    if not ours_parent or not chrome_parent:
        return fields
    op, cp = ours_parent.get("style") or {}, chrome_parent.get("style") or {}
    out = []
    for f in fields:
        p = f.get("property")
        if (p in INHERITED
                and normalize_value(p, op.get(p)) == normalize_value(p, f["ours"])
                and normalize_value(p, cp.get(p)) == normalize_value(p, f["chrome"])):
            continue
        out.append(f)
    return out


def _same_element(o: Dict[str, Any], c: Dict[str, Any], text_tolerance: int) -> bool:
    """Could these two records be the same element? Tag, id and classes
    equal, text length within the DOM tolerance. Text itself is never read."""
    oi, ci = o["identity"], c["identity"]
    if (oi["tag"], oi.get("id"), list(oi.get("classes") or [])) != (ci["tag"], ci.get("id"), list(ci.get("classes") or [])):
        return False
    ol, cl = (o.get("tree") or {}).get("text_length"), (c.get("tree") or {}).get("text_length")
    return ol is None or cl is None or abs(ol - cl) <= max(text_tolerance, TEXT_TOLERANCE_SHARE * cl)


def _alignment(a: List[Any], b: List[Any], eq, leftmost: bool) -> Dict[int, int]:
    """One longest alignment of a onto b (indices), taking matches as early
    (leftmost) or as late as possible."""
    if not leftmost:
        n, m = len(a), len(b)
        rev = _alignment(a[::-1], b[::-1], eq, True)
        return {n - 1 - i: m - 1 - j for i, j in rev.items()}
    n, m = len(a), len(b)
    suf = [[0] * (m + 1) for _ in range(n + 1)]
    for i in range(n - 1, -1, -1):
        for j in range(m - 1, -1, -1):
            suf[i][j] = max(suf[i + 1][j], suf[i][j + 1], suf[i + 1][j + 1] + 1 if eq(a[i], b[j]) else 0)
    out, i, j = {}, 0, 0
    while i < n and j < m:
        if eq(a[i], b[j]) and suf[i][j] == suf[i + 1][j + 1] + 1:
            out[i] = j
            i, j = i + 1, j + 1
        elif suf[i][j + 1] >= suf[i + 1][j]:
            j += 1
        else:
            i += 1
    return out


def _realign(pairs, chrome_only, ours_only, chrome, ours, chrome_parent, ours_parent, text_tolerance):
    """Re-pair the children of a paired parent that has an unpaired child.

    The oracle key numbers same-tag siblings (`:nth-of-type`), so removing
    one shifts the keys of those after it and they pair with the wrong
    elements. Under such a parent, same-tag children are aligned in order
    on (tag, id, classes, text length). A pair is kept only when the
    leftmost and the rightmost longest alignments agree on it; an element
    they disagree on is UNVERIFIED (reported, never classified), so a
    removed one of identical siblings is never named as another. Paired
    elements' descendants are re-paired by their path below them.
    Returns (pairs, chrome_only, ours_only, unverified chrome orders,
    unverified ours orders).
    """
    c_partner = {c["order"]: o for _, o, c in pairs}
    o_partner = {o["order"]: c for _, o, c in pairs}
    gaps = set()
    for c in chrome_only:
        p = chrome_parent.get(c["order"])
        if p and p["order"] in c_partner:
            gaps.add((c_partner[p["order"]]["order"], p["order"]))
    for o in ours_only:
        p = ours_parent.get(o["order"])
        if p and p["order"] in o_partner:
            gaps.add((p["order"], o_partner[p["order"]]["order"]))
    if not gaps:
        return pairs, chrome_only, ours_only, set(), set()

    def kids(es, parent_of):
        out: Dict[int, List[Dict[str, Any]]] = {}
        for e in es:
            p = parent_of.get(e["order"])
            if p:
                out.setdefault(p["order"], []).append(e)
        return out

    c_kids, o_kids = kids(chrome, chrome_parent), kids(ours, ours_parent)

    def subtree(e, kid_map):
        out, stack = [], [e]
        while stack:
            x = stack.pop()
            out.append(x)
            stack.extend(kid_map.get(x["order"], []))
        return out

    eq = lambda o, c: _same_element(o["record"], c["record"], text_tolerance)  # noqa: E731
    new_pairs, c_reset, o_reset = [], set(), set()
    c_unverified, o_unverified = set(), set()
    for po, pc in sorted(gaps):
        tags = {e["tag"] for e in c_kids.get(pc, []) + o_kids.get(po, [])}
        for tag in sorted(tags):
            cs = [e for e in c_kids.get(pc, []) if e["tag"] == tag]
            os_ = [e for e in o_kids.get(po, []) if e["tag"] == tag]
            if len(cs) == len(os_) and all(c["order"] in c_partner and c_partner[c["order"]] in os_ for c in cs):
                continue  # already paired among themselves
            for e in cs:
                c_reset.update(x["order"] for x in subtree(e, c_kids))
            for e in os_:
                o_reset.update(x["order"] for x in subtree(e, o_kids))
            left = _alignment(os_, cs, eq, True)
            right = _alignment(os_, cs, eq, False)
            sure = {i: j for i, j in left.items() if right.get(i) == j}
            unsure_o = (set(left) | set(right)) - set(sure)
            unsure_c = (set(left.values()) | set(right.values())) - set(sure.values())
            for i in unsure_o:
                o_unverified.update(x["order"] for x in subtree(os_[i], o_kids))
            for j in unsure_c:
                c_unverified.update(x["order"] for x in subtree(cs[j], c_kids))
            for i, j in sure.items():
                o_top, c_top = os_[i], cs[j]
                o_base, c_base = o_top["record"]["identity"]["selector"], c_top["record"]["identity"]["selector"]
                o_rel = {x["record"]["identity"]["selector"][len(o_base):]: x for x in subtree(o_top, o_kids)}
                for x in subtree(c_top, c_kids):
                    y = o_rel.get(x["record"]["identity"]["selector"][len(c_base):])
                    if y is not None:
                        new_pairs.append(("aligned", y, x))
    kept = [p for p in pairs if p[1]["order"] not in o_reset and p[2]["order"] not in c_reset]
    pairs = sorted(kept + new_pairs, key=lambda p: p[2]["order"])
    paired_c = {c["order"] for _, _, c in pairs}
    paired_o = {o["order"] for _, o, _ in pairs}
    chrome_only = [c for c in chrome if c["order"] not in paired_c and c["order"] not in c_unverified
                   and (c["order"] in c_reset or any(c is x for x in chrome_only))]
    ours_only = [o for o in ours if o["order"] not in paired_o and o["order"] not in o_unverified
                 and (o["order"] in o_reset or any(o is x for x in ours_only))]
    return pairs, chrome_only, ours_only, c_unverified, o_unverified


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
    ours_text, chrome_text = page_text_lengths(ours), page_text_lengths(chrome)
    pairs, chrome_only, ours_only, c_unverified, o_unverified = _realign(
        pairs, chrome_only, ours_only, chrome, ours, chrome_parent, ours_parent, text_tolerance)
    partner = {c["order"]: (key, o) for key, o, c in pairs}
    chrome_missing = {c["order"] for c in chrome_only}
    ours_extra = {o["order"] for o in ours_only}
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
                ("DOM", lambda: dom_diffs(orec, rec, text_tolerance,
                                          (ours_text[o["order"]], chrome_text[c["order"]]))),
                ("STYLE", lambda: style_diffs(orec, rec, par["record"] if par else None)),
                ("GEOMETRY", lambda: geometry_diffs(orec, rec, threshold)),
            ):
                d = fn()
                if d:
                    found = (stage, d)
                    break
        elif c["selector"] in ambiguous_keys or c["order"] in c_unverified:
            stage_of[c["order"]] = None  # unpaired by design: not a divergence
            continue
        else:
            found = ("DOM", [{"field": "element", "ours": "missing", "chrome": "present"}])
        stage_of[c["order"]] = found[0] if found else None
        if not found:
            continue
        pstage = stage_of.get(par["order"]) if par else None
        # Not a root under a parent that diverges at an earlier stage; a
        # same-stage consequence is settled below, once every delta is known.
        is_root = pstage is None or STAGE_RANK[pstage] >= STAGE_RANK[found[0]]
        if c["order"] in chrome_missing and par and par["order"] in chrome_missing:
            is_root = False  # inside a missing element: missing with it
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

    # Same-stage consequences are not roots.
    chrome_by_order = {c["order"]: c for c in chrome}
    by_selector = {c["record"]["identity"]["selector"]: c["order"] for c in chrome}
    children: Dict[int, List[int]] = {}
    for c in chrome:
        p = chrome_parent[c["order"]]
        if p:
            children.setdefault(p["order"], []).append(c["order"])
    prev_sibling = {}
    for kids in children.values():
        for a, b in zip(kids, kids[1:]):
            prev_sibling[b] = a
    boxes = _Boxes(partner, chrome_by_order)
    for d in divergent:
        if not d["root"]:
            continue
        order, par = d["order"], d["_parent_order"]
        # Only a border-box difference can be inherited from neighbours: a
        # content-box one under an unchanged border box is the element's own.
        if d["stage"] == "GEOMETRY" and all(f.get("field") == "border_box" for f in d["fields"]):
            fc = d["formatting_context"] or ""
            cb = ((chrome_by_order[order]["record"].get("box") or {}).get("containing_block") or {}).get("selector")
            cb_order = by_selector.get(cb) if fc.startswith("absolute") and cb else None
            if _geometry_explained(order, boxes, par, prev_sibling.get(order), children.get(order, []),
                                   cb_order, threshold):
                d["root"] = False
        elif d["stage"] == "STYLE" and par is not None and par in partner:
            own = _inherited_only(d["fields"], partner[par][1]["record"], chrome_by_order[par]["record"])
            if not own:
                d["root"] = False
            else:
                d["fields"] = own

    # Extra elements: ours only. A root unless their parent's partner diverges earlier.
    chrome_order_of_ours = {o["order"]: c["order"] for _, o, c in pairs}
    for o in ours_only:
        if o["selector"] in ambiguous_keys or o["order"] in o_unverified or skip(o):
            continue
        rec = o["record"]
        par = ours_parent[o["order"]]
        pc = chrome_order_of_ours.get(par["order"]) if par else None
        inside_extra = bool(par and par["order"] in ours_extra)
        obox = _rect((rec.get("box") or {}).get("border_box"))
        divergent.append({
            "order": None,
            "stage": "DOM",
            # DOM is the earliest stage: an extra element is a root, unless
            # it is inside another extra element.
            "root": not inside_extra,
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
            "unverified": len(c_unverified) + len(o_unverified),
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
    try:
        if args.out:
            Path(args.out).write_text(text + "\n")
            print(format_report(result))
        else:
            print(text)
        sys.stdout.flush()
    except BrokenPipeError:
        # The reader (head, a pager) stopped early: not an error of ours.
        devnull = os.open(os.devnull, os.O_WRONLY)
        os.dup2(devnull, sys.stdout.fileno())
    return 0


if __name__ == "__main__":
    sys.exit(main())
