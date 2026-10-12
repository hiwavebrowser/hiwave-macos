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

A divergence ROOT is a divergent element that no other divergence explains:
not a child of an element that diverges at an earlier stage, not a box that
only follows its parent's box, not a value inherited unchanged from its
parent, and not an ancestor whose only difference is what its descendants
make of it. See README.md for the rules and the JSON contract.

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
from difflib import SequenceMatcher
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
    # The engine maps start/end to flex-start/flex-end when it parses them.
    "justify-content": {"normal": "flex-start", "start": "flex-start", "end": "flex-end"},
    "align-items": {"normal": "stretch", "start": "flex-start", "end": "flex-end"},
    # The engine stores auto as 0 and cannot tell them apart.
    "z-index": {"0": "auto"},
    "align-self": {"normal": "auto"},
    "justify-self": {"normal": "auto"},
    "text-align": {"start": "left", "end": "right", "-webkit-left": "left"},
    "font-weight": {"normal": "400", "bold": "700"},
    "min-width": {"auto": "0px"},
    "min-height": {"auto": "0px"},
    "display": {"list-item": "block"},
    "background-color": {"transparent": "rgba(0, 0, 0, 0)"},
}

GAP_PROPS = {"gap", "row-gap", "column-gap"}
INHERITED = {"color", "font-family", "font-size", "font-weight", "line-height", "white-space",
             "text-align", "visibility"}
REPLACED_TAGS = {"img", "video", "canvas", "iframe", "embed", "object", "svg", "input", "textarea",
                 "select", "button"}
URL_RE = re.compile(r"""url\(\s*(["']?)(.*?)\1\s*\)""")
GRADIENT_RE = re.compile(r"(?:-webkit-|-moz-)?((?:repeating-)?(?:linear|radial|conic)-gradient)\(.*?\)(?=,\s*(?:url|(?:-webkit-|-moz-)?(?:repeating-)?(?:linear|radial|conic)-gradient)|$)")
MATRIX3D_RE = re.compile(r"matrix3d\(([^)]*)\)")
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
    if prop in GAP_PROPS:
        v = " ".join("0px" if t == "normal" else t for t in v.split())
        parts = v.split()
        if prop == "gap" and len(parts) == 2 and parts[0] == parts[1]:
            v = parts[0]
    if prop == "background-image":
        # Chromium resolves URLs; the engine keeps the specified text. Compare
        # the last path segment, and gradients by kind.
        v = URL_RE.sub(lambda m: 'url("%s")' % m.group(2).split("?")[0].rstrip("/").rsplit("/", 1)[-1], v)
        v = GRADIENT_RE.sub(lambda m: m.group(1).lower() + "(…)", v)
        v = re.sub(r"(?:-webkit-|-moz-)((?:repeating-)?(?:linear|radial|conic)-gradient)", r"\1", v)
        return v
    if prop == "transform":
        m3 = MATRIX3D_RE.fullmatch(v)
        if m3:
            n = [float(x) for x in m3.group(1).split(",")]
            flat = [n[2], n[3], n[6], n[7], n[8], n[9], n[11], n[14]] == [0, 0, 0, 0, 0, 0, 0, 0] and n[10] == 1 and n[15] == 1
            if len(n) == 16 and flat:
                v = "matrix(%s)" % ", ".join(str(x) for x in (n[0], n[1], n[4], n[5], n[12], n[13]))
        nums = [float(x) for x in NUM_RE.findall(v)] if v.startswith("matrix(") else None
        if nums and len(nums) == 6 and all(abs(a - b) <= 1e-6 for a, b in zip(nums, (1, 0, 0, 1, 0, 0))):
            return "none"
        return NUM_RE.sub(lambda m: _num(m.group(0)), v)
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
    tag = rec["identity"]["tag"]
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
        # float and clear do nothing on a flex or grid item; vertical-align
        # only moves inline-level boxes and table cells; object-fit only
        # replaced elements.
        if p in ("float", "clear") and (pflex or pgrid):
            continue
        if p == "vertical-align" and (pflex or pgrid or not (display.startswith("inline") or display == "table-cell")):
            continue
        if p == "object-fit" and tag not in REPLACED_TAGS:
            continue
        out.append(p)
    return out


def _unresolved_track_list(v: str) -> bool:
    """Only plain px track lists compare: the engine has no resolved tracks
    (it reports the specified list, or `none` for implicit-only grids), and
    Chromium reports the resolved px tracks of a grid container."""
    return v == "none" or bool(re.search(r"fr|auto|minmax|repeat|%|content", v))


def style_diffs(ours: Dict[str, Any], chrome: Dict[str, Any], chrome_parent: Optional[Dict[str, Any]]) -> List[Dict[str, Any]]:
    out = []
    os_, cs = ours.get("style") or {}, chrome.get("style") or {}
    for p in comparable_props(chrome, chrome_parent):
        o, c = normalize_value(p, os_.get(p)), normalize_value(p, cs.get(p))
        if o is None or c is None or o == c:
            continue  # unknown on one side is not a difference
        grid = (cs.get("display") or "") in ("grid", "inline-grid")
        if p.startswith("grid-template") and grid and (_unresolved_track_list(o) or _unresolved_track_list(c)):
            continue
        if p == "justify-content" and grid and "normal" in (os_.get(p), cs.get(p)):
            continue  # normal behaves as stretch in a grid, as flex-start in a flexbox
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


def _signature(rec: Dict[str, Any]) -> Tuple:
    ident, tree = rec["identity"], rec.get("tree") or {}
    return (ident["tag"], ident.get("id"), tuple(ident.get("classes") or ()), tree.get("text_length"))


def _children(es: List[Dict[str, Any]], parent: Dict[int, Optional[Dict[str, Any]]]) -> Dict[Optional[int], List[Dict[str, Any]]]:
    out: Dict[Optional[int], List[Dict[str, Any]]] = {}
    for e in es:
        p = parent[e["order"]]
        out.setdefault(p["order"] if p else None, []).append(e)
    return out


def realign(c2o: Dict[int, Dict[str, Any]], o2c: Dict[int, Dict[str, Any]],
            chrome_kids: Dict, ours_kids: Dict, keys: Dict[int, str], pc: int, po: int) -> None:
    """Re-pair two paired parents' element children by sequence alignment.

    The structural keys pair children by position (`:nth-of-type`), so one
    removed or added sibling shifts every later sibling onto its neighbour's
    partner. Children are aligned on (tag, id, classes, text length); within
    a gap of equal length, same-tag children pair in order (the same element
    with changed text). Each new pair's subtree is realigned the same way.
    """
    C, O = chrome_kids.get(pc, []), ours_kids.get(po, [])
    pairs: List[Tuple[Dict[str, Any], Dict[str, Any]]] = []
    sm = SequenceMatcher(None, [_signature(c["record"]) for c in C], [_signature(o["record"]) for o in O], autojunk=False)
    for tag, i1, i2, j1, j2 in sm.get_opcodes():
        if tag == "equal" or (tag == "replace" and i2 - i1 == j2 - j1):
            for c, o in zip(C[i1:i2], O[j1:j2]):
                if c["record"]["identity"]["tag"] == o["record"]["identity"]["tag"]:
                    pairs.append((c, o))
    before = {c["order"]: c2o.get(c["order"], {}).get("order") for c in C}
    for c in C:
        old = c2o.pop(c["order"], None)
        if old is not None:
            o2c.pop(old["order"], None)
    for o in O:
        old = o2c.pop(o["order"], None)
        if old is not None:
            c2o.pop(old["order"], None)
    for c, o in pairs:
        c2o[c["order"]], o2c[o["order"]] = o, c
        if before.get(c["order"]) != o["order"]:
            keys[c["order"]] = "aligned"  # the structural key had paired it elsewhere
        realign(c2o, o2c, chrome_kids, ours_kids, keys, c["order"], o["order"])


def _delta(o: Optional[Dict[str, float]], c: Optional[Dict[str, float]]) -> Optional[Dict[str, float]]:
    if o is None or c is None:
        return None
    return {k: o[k] - c[k] for k in ("x", "y", "w", "h")}


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
    pairs, _, _, ambiguous = ed.match(ours, chrome)
    ambiguous_keys = {a["key"] for a in ambiguous}

    # Tree parents from each side's own unique structural selectors.
    def parents(es):
        by_sel = {e["record"]["identity"]["selector"]: e for e in es}
        out = {}
        for e in es:
            sel = e["record"]["identity"]["selector"]
            out[e["order"]] = by_sel.get(sel.rsplit(" > ", 1)[0]) if " > " in sel else None
        return out

    chrome_parent, ours_parent = parents(chrome), parents(ours)
    chrome_kids, ours_kids = _children(chrome, chrome_parent), _children(ours, ours_parent)
    c2o = {c["order"]: o for _, o, c in pairs}
    o2c = {o["order"]: c for _, o, c in pairs}
    keys = {c["order"]: k for k, _, c in pairs}

    # Parents whose children did not pair one-to-one under them: realign.
    suspects = set()
    for c in chrome:
        pc = chrome_parent[c["order"]]
        o = c2o.get(c["order"])
        if o is None:
            if pc is not None and c["selector"] not in ambiguous_keys:
                suspects.add(pc["order"])
            continue
        po = ours_parent[o["order"]]
        if pc is not None and (po is None or o2c.get(po["order"], {}).get("order") != pc["order"]):
            suspects.add(pc["order"])
    for o in ours:
        if o["order"] not in o2c and o["selector"] not in ambiguous_keys:
            po = ours_parent[o["order"]]
            if po is not None and po["order"] in o2c:
                suspects.add(o2c[po["order"]]["order"])
    for pc in sorted(suspects):
        po = c2o.get(pc)
        if po is not None:
            realign(c2o, o2c, chrome_kids, ours_kids, keys, pc, po["order"])

    stage_of: Dict[int, Optional[str]] = {}
    info: Dict[int, Dict[str, Any]] = {}
    divergent: List[Dict[str, Any]] = []

    def box_of(e, field="border_box"):
        return _rect(((e["record"].get("box") or {}).get(field))) if e else None

    for c in chrome:
        rec = c["record"]
        par = chrome_parent[c["order"]]
        if skip(c) or (c["order"] not in c2o and c["selector"] in ambiguous_keys):
            stage_of[c["order"]] = None  # head, or unpaired by design: not classified
            continue
        o = c2o.get(c["order"])
        found: Optional[Tuple[str, List[Dict[str, Any]]]] = None
        if o is not None:
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
        else:
            found = ("DOM", [{"field": "element", "ours": "missing", "chrome": "present"}])
        stage_of[c["order"]] = found[0] if found else None
        if not found:
            continue
        cbox, obox = box_of(c), box_of(o)
        area = ed.mismatch_area(obox, cbox) if found[0] == "GEOMETRY" and cbox and obox else max(_area(cbox), _area(obox))
        d = {
            "order": c["order"],
            "stage": found[0],
            "root": True,
            "attributed_to": None,
            "selector": rec["identity"]["selector"],
            "oracle_key": rec["identity"]["oracle_key"],
            "tag": rec["identity"]["tag"],
            "match_key": keys.get(c["order"]) if o is not None else None,
            "formatting_context": (rec.get("box") or {}).get("formatting_context"),
            "affected_area": round(area, 2) + 0.0,
            "fields": found[1],
            "_parent_order": par["order"] if par else None,
        }
        info[c["order"]] = d
        divergent.append(d)

    # Extra elements: ours only, placed after their parent's partner.
    for o in ours:
        if o["order"] in o2c or o["selector"] in ambiguous_keys or skip(o):
            continue
        rec = o["record"]
        par = ours_parent[o["order"]]
        pc = o2c[par["order"]]["order"] if par and par["order"] in o2c else None
        divergent.append({
            "order": None,
            "stage": "DOM",
            "root": True,
            "attributed_to": None,
            "selector": rec["identity"]["selector"],
            "oracle_key": rec["identity"]["oracle_key"],
            "tag": rec["identity"]["tag"],
            "match_key": None,
            "formatting_context": (rec.get("box") or {}).get("formatting_context"),
            "affected_area": round(_area(box_of(o)), 2) + 0.0,
            "fields": [{"field": "element", "ours": "present", "chrome": "missing"}],
            "_parent_order": pc,
        })

    # --- Attribution: which divergences are consequences of another.
    by_order = {c["order"]: c for c in chrome}

    def follows_parent(d) -> bool:
        """Position and width only follow the parent: shifted by the parent's
        content box (plus the size changes of earlier siblings), and no wider
        or narrower than the parent's content-box change allows. Heights are
        not inherited from a parent, so the height is judged separately."""
        f = d["fields"][0]
        if "dx" not in f:
            return False
        pc = d["_parent_order"]
        near = lambda a, b: abs(a - b) <= threshold  # noqa: E731
        if pc is None or stage_of.get(pc) is None:
            sib = (0.0, 0.0)
            if pc is not None:
                sib = sibling_shift(pc, d["order"])
            return (near(f["dx"], 0) or near(f["dx"], sib[0])) and (near(f["dy"], 0) or near(f["dy"], sib[1])) \
                and abs(f["dw"]) <= threshold
        if stage_of.get(pc) != "GEOMETRY":
            return False
        p_c, p_o = by_order[pc], c2o.get(pc)
        pd = _delta(box_of(p_o, "content_box"), box_of(p_c, "content_box"))
        if pd is None:
            return False
        sib_dw, sib_dh = sibling_shift(pc, d["order"])
        dx_ok = any(near(f["dx"], v) for v in (pd["x"], pd["x"] + sib_dw, pd["x"] + pd["w"] / 2, pd["x"] + pd["w"]))
        dy_ok = any(near(f["dy"], v) for v in (pd["y"], pd["y"] + sib_dh))
        dw_ok = abs(f["dw"]) <= threshold or (pd["w"] * f["dw"] > 0 and abs(f["dw"]) <= abs(pd["w"]) + threshold)
        return dx_ok and dy_ok and dw_ok

    def sibling_shift(pc, order) -> Tuple[float, float]:
        """Summed width and height changes of the earlier siblings."""
        sib_dw = sib_dh = 0.0
        for s in chrome_kids.get(pc, []):
            if s["order"] >= order:
                break
            sd = _delta(box_of(c2o.get(s["order"])), box_of(s))
            if sd:
                sib_dw, sib_dh = sib_dw + sd["w"], sib_dh + sd["h"]
        return sib_dw, sib_dh

    def explained_geometry(d) -> bool:
        f = d["fields"][0]
        return stage_of.get(d["_parent_order"]) == "GEOMETRY" and follows_parent(d) \
            and abs(f["dh"]) <= threshold

    def explained_style(d) -> List[Dict[str, Any]]:
        """The differing properties not inherited unchanged from the parent."""
        pc = d["_parent_order"]
        if pc is None or pc not in c2o:
            return d["fields"]
        pcs, pos = by_order[pc]["record"].get("style") or {}, c2o[pc]["record"].get("style") or {}
        rest = []
        for f in d["fields"]:
            p = f["property"]
            inherited = (p in INHERITED
                         and normalize_value(p, pcs.get(p)) == normalize_value(p, f["chrome"])
                         and normalize_value(p, pos.get(p)) == normalize_value(p, f["ours"]))
            if not inherited:
                rest.append(f)
        return rest

    for d in divergent:
        pc = d["_parent_order"]
        if d["order"] is None or pc is None:
            continue
        pstage = stage_of.get(pc)
        parent_sel = by_order[pc]["record"]["identity"]["selector"]
        if pstage is not None and STAGE_RANK[pstage] < STAGE_RANK[d["stage"]]:
            d["root"], d["attributed_to"] = False, parent_sel  # an earlier stage above explains it
        elif d["stage"] == "GEOMETRY" and explained_geometry(d):
            d["root"], d["attributed_to"] = False, parent_sel
        elif d["stage"] == "STYLE" and pstage == "STYLE":
            rest = explained_style(d)
            if not rest:
                d["root"], d["attributed_to"] = False, parent_sel
            else:
                d["fields"] = rest

    # Bottom-up: an ancestor whose only differences are what its descendants
    # make of it (child count and text for DOM, height for GEOMETRY).
    def descendant_roots(order, stage):
        found = []
        stack = list(chrome_kids.get(order, []))
        while stack:
            e = stack.pop()
            d = info.get(e["order"])
            if d and d["root"] and d["stage"] == stage:
                found.append(d)
            stack.extend(chrome_kids.get(e["order"], []))
        return found

    extras_under = {d["_parent_order"] for d in divergent if d["order"] is None}
    for d in sorted((d for d in divergent if d["order"] is not None), key=lambda d: -d["order"]):
        if not d["root"]:
            continue
        if d["stage"] == "DOM" and all(f["field"] in ("child_count", "text_length") for f in d["fields"]):
            below = descendant_roots(d["order"], "DOM")
            if below or d["order"] in extras_under:
                d["root"] = False
                d["attributed_to"] = below[0]["selector"] if below else "an extra child"
        elif d["stage"] == "GEOMETRY":
            # Height from its content, position and width from above.
            if follows_parent(d):
                below = descendant_roots(d["order"], "GEOMETRY")
                if below:
                    d["root"], d["attributed_to"] = False, below[0]["selector"]

    def doc_key(d):
        if d["order"] is not None:
            return (d["order"], 0)
        return ((d["_parent_order"] if d["_parent_order"] is not None else math.inf), 1)

    roots = sorted((d for d in divergent if d["root"]), key=doc_key)
    first = roots[0] if roots else None

    def has_root_below(d):
        if d["order"] is None:
            return False
        for other in roots:
            p = other["_parent_order"]
            while p is not None:
                if p == d["order"] and other["stage"] == d["stage"]:
                    return True
                pe = chrome_parent.get(p)
                p = pe["order"] if pe else None
        return False

    innermost = next((d for d in roots if not has_root_below(d)), None)

    def public(d):
        return None if d is None else {k: v for k, v in d.items() if not k.startswith("_")}

    ranked = sorted(roots, key=lambda d: (-d["affected_area"],) + doc_key(d))
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
    chrome_only = [c for c in chrome if c["order"] not in c2o and c["selector"] not in ambiguous_keys]
    ours_only = [o for o in ours if o["order"] not in o2c and o["selector"] not in ambiguous_keys]

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
            "matched": len(c2o),
            "realigned": sum(1 for k in keys.values() if k == "aligned"),
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
            "consequences": len(divergent) - len(roots),
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
    try:
        sys.exit(main())
    except BrokenPipeError:  # stdout piped to `head`
        sys.stderr.close()
        sys.exit(0)
