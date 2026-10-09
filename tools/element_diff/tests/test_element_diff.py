"""Unit tests for element_diff: matcher, scoring and summary. Synthetic JSON only."""

import json
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import element_diff as ed  # noqa: E402


def box(selector, x, y, w, h, tag=None, children=None, **extra):
    node = {
        "type": "block",
        "selector": selector,
        "tag": tag or ed.describe_selector(selector)["tag"],
        "border_box": {"x": x, "y": y, "width": w, "height": h},
    }
    node.update(extra)
    if children:
        node["children"] = children
    return node


def ours_doc(*children, font_derived=True):
    return {
        "version": 1,
        "viewport": {"width": 400, "height": 300},
        "text_backend": "coretext" if font_derived else "stub-0.5em",
        "text_metrics_font_derived": font_derived,
        "root": {"type": "block", "children": list(children)},
    }


def chrome_el(selector, x, y, w, h, **extra):
    el = {
        "selector": selector,
        "tag": ed.describe_selector(selector)["tag"] or "div",
        "rect": {"x": x, "y": y, "width": w, "height": h},
    }
    el.update(extra)
    return el


def chrome_doc(*els):
    return {"viewport": {"width": 400, "height": 300}, "elements": list(els)}


# --- selector parsing -------------------------------------------------------


def test_parse_segment_keeps_colons_and_spaces_in_classes():
    assert ed.parse_segment("div.md:flex:nth-of-type(3)") == ("div", "md:flex", 3)
    assert ed.parse_segment("div.card featured:nth-of-type(2)") == ("div", "card featured", 2)
    assert ed.parse_segment("p") == ("p", None, None)


def test_describe_selector():
    assert ed.describe_selector("#main") == {"tag": None, "id": "main", "class": None}
    d = ed.describe_selector("body > div.a.b:nth-of-type(2) > span.x")
    assert d == {"tag": "span", "id": None, "class": "x"}
    assert ed.describe_selector("body > div.a.b:nth-of-type(2)")["class"] == "a b"


def test_strip_classes():
    assert (
        ed.strip_classes("body > div.card featured:nth-of-type(2) > p.lead")
        == "body > div:nth-of-type(2) > p"
    )
    assert ed.strip_classes("#x") == "#x"


# --- loading ----------------------------------------------------------------


def test_load_ours_skips_anonymous_text_skipped_tags_and_zero_size():
    doc = ours_doc(
        box("html > body", 0, 0, 400, 300, children=[
            {"type": "anonymous_block", "border_box": {"x": 0, "y": 0, "width": 9, "height": 9}},
            {"type": "text", "text": "hi", "rect": {"x": 0, "y": 0, "width": 9, "height": 9}},
            box("body > style", 0, 0, 5, 5, tag="style"),
            box("body > div.empty", 0, 0, 0, 0),
            box("body > div.keep", 1, 2, 3, 4),
        ]),
    )
    ours, skipped = ed.load_ours(doc)
    assert [o["selector"] for o in ours] == ["html > body", "body > div.keep"]
    assert skipped == 2


def test_load_ours_prefers_visual_then_fragment_union_rect():
    node = box("body > span", 0, 0, 10, 10,
               visual_border_box={"x": 5, "y": 5, "width": 10, "height": 10},
               fragment_union_border_box={"x": 1, "y": 1, "width": 1, "height": 1})
    ours, _ = ed.load_ours(ours_doc(node))
    assert ours[0]["rect"] == {"x": 5.0, "y": 5.0, "w": 10.0, "h": 10.0}


def test_load_chrome_uses_id_and_classname_when_present():
    doc = chrome_doc(
        chrome_el("body > div.a", 0, 0, 1, 1, id=None, className="a extra"),
        chrome_el("body > p", 0, 0, 1, 1),
    )
    c = ed.load_chrome(doc)
    assert c[0]["class"] == "a extra"
    assert c[1]["class"] is None and c[1]["tag"] == "p"


# --- matching ---------------------------------------------------------------


def test_match_prefers_id_then_path_then_classless_path():
    ours = ours_doc(
        box("#hero", 0, 0, 10, 10, tag="section"),
        box("body > div.a:nth-of-type(1)", 0, 0, 10, 10),
        # class differs on our side (script added "js-ready" only in Chrome)
        box("body > div.nav:nth-of-type(2)", 0, 0, 10, 10),
    )
    chrome = chrome_doc(
        chrome_el("#hero", 0, 0, 10, 10, tag="section", id="hero"),
        chrome_el("body > div.a:nth-of-type(1)", 0, 0, 10, 10),
        chrome_el("body > div.nav js-ready:nth-of-type(2)", 0, 0, 10, 10),
    )
    pairs, c_only, o_only, amb = ed.match(ed.load_ours(ours)[0], ed.load_chrome(chrome))
    assert [k for k, _, _ in pairs] == ["id", "path", "path_noclass"]
    assert not c_only and not o_only and not amb


def test_match_reports_unmatched_both_ways():
    ours = ours_doc(box("body > div.only-ours", 0, 0, 5, 5), box("body > p", 0, 0, 5, 5))
    chrome = chrome_doc(chrome_el("body > p", 0, 0, 5, 5), chrome_el("body > ul", 0, 0, 5, 5))
    pairs, c_only, o_only, _ = ed.match(ed.load_ours(ours)[0], ed.load_chrome(chrome))
    assert len(pairs) == 1
    assert [e["selector"] for e in c_only] == ["body > ul"]
    assert [e["selector"] for e in o_only] == ["body > div.only-ours"]


def test_duplicate_selector_is_ambiguous_not_first_matched():
    ours = ours_doc(box("body > span", 0, 0, 5, 5), box("body > span", 50, 0, 5, 5))
    chrome = chrome_doc(chrome_el("body > span", 0, 0, 5, 5))
    pairs, c_only, o_only, amb = ed.match(ed.load_ours(ours)[0], ed.load_chrome(chrome))
    assert pairs == [] and c_only == [] and o_only == []
    assert amb == [{"key": "body > span", "ours_count": 2, "chrome_count": 1}]


def test_classless_fallback_refuses_non_unique_key():
    # Both sides strip to "body > div"; ours has two, so no pairing.
    ours = ours_doc(box("body > div.x", 0, 0, 5, 5), box("body > div.y", 0, 0, 5, 5))
    chrome = chrome_doc(chrome_el("body > div.z", 0, 0, 5, 5))
    pairs, c_only, o_only, _ = ed.match(ed.load_ours(ours)[0], ed.load_chrome(chrome))
    assert pairs == [] and len(c_only) == 1 and len(o_only) == 2


def test_pairs_follow_chrome_document_order():
    ours = ours_doc(box("body > p.b", 0, 0, 5, 5), box("body > p.a", 0, 0, 5, 5))
    chrome = chrome_doc(chrome_el("body > p.a", 0, 0, 5, 5), chrome_el("body > p.b", 0, 0, 5, 5))
    pairs, *_ = ed.match(ed.load_ours(ours)[0], ed.load_chrome(chrome))
    assert [c["selector"] for _, _, c in pairs] == ["body > p.a", "body > p.b"]


# --- scoring and summary ----------------------------------------------------


def test_mismatch_area():
    a = {"x": 0, "y": 0, "w": 10, "h": 10}
    assert ed.mismatch_area(a, a) == 0
    assert ed.mismatch_area(a, {"x": 1, "y": 0, "w": 10, "h": 10}) == 20
    assert ed.mismatch_area(a, {"x": 50, "y": 50, "w": 10, "h": 10}) == 200


def test_element_record_deltas_are_ours_minus_chrome():
    rec = ed.element_record(
        "path",
        {"selector": "body > p", "tag": "p", "rect": {"x": 3, "y": 0, "w": 10, "h": 8}},
        {"selector": "body > p", "tag": "p", "id": None, "class": "c",
         "rect": {"x": 0, "y": 1, "w": 12, "h": 8}},
    )
    assert (rec["dx"], rec["dy"], rec["dw"], rec["dh"]) == (3, -1, -2, 0)
    assert rec["max_error"] == 3
    assert rec["ours"] == {"x": 3, "y": 0, "w": 10, "h": 8}
    assert rec["class"] == "c"


def build_page():
    """body > header (clean) > h1 (6px tall wrong) ; body > main (shifted 10px)."""
    ours = ours_doc(
        box("html > body", 0, 0, 400, 236, children=[
            box("body > header", 0, 0, 400, 40, children=[box("body > header > h1", 0, 0, 400, 34)]),
            box("body > main", 0, 50, 400, 100),
            box("body > footer", 0, 200, 400, 21.5),
        ]),
    )
    chrome = chrome_doc(
        chrome_el("html > body", 0, 0, 400, 240),
        chrome_el("body > header", 0, 0, 400, 40),
        chrome_el("body > header > h1", 0, 0, 400, 40),
        chrome_el("body > main", 0, 40, 400, 100),
        chrome_el("body > footer", 0, 200, 400, 20),
        chrome_el("body > aside", 0, 0, 50, 50),
    )
    return ours, chrome


def test_summary_counts_share_and_first_divergence():
    ours, chrome = build_page()
    s = ed.compare(ours, chrome)["summary"]
    assert s["chrome_elements"] == 6 and s["ours_elements"] == 5
    assert s["matched"] == 5 and s["chrome_only"] == 1 and s["ours_only"] == 0
    # within 2px: header (0) and footer (1.5)
    assert s["within_tolerance"] == 2
    assert s["within_tolerance_share"] == pytest.approx(0.4)
    # over 4px: body (dh -4 is NOT over), h1 (dh -6), main (dy +10)
    assert s["over_threshold"] == 2
    assert s["first_over_threshold"]["path"] == "body > header > h1"
    assert s["first_over_threshold_non_ancestor"]["path"] == "body > header > h1"


def test_first_non_ancestor_skips_wrappers_with_bad_descendants():
    ours = ours_doc(box("html > body", 0, 0, 400, 100, children=[
        box("body > div", 0, 0, 400, 100, children=[box("body > div > p", 0, 0, 400, 100)]),
    ]))
    chrome = chrome_doc(
        chrome_el("html > body", 0, 0, 400, 120),
        chrome_el("body > div", 0, 0, 400, 120),
        chrome_el("body > div > p", 0, 0, 400, 120),
    )
    s = ed.compare(ours, chrome)["summary"]
    assert s["first_over_threshold"]["path"] == "html > body"
    assert s["first_over_threshold_non_ancestor"]["path"] == "body > div > p"


def test_first_non_ancestor_sees_through_id_wrapper():
    # #wrapper is over threshold only because the p inside it is.
    ours = ours_doc(box("html > body", 0, 0, 400, 100, children=[
        box("#wrapper", 0, 0, 400, 100, tag="div", children=[
            box("body > div:nth-of-type(1) > p", 0, 0, 400, 100),
        ]),
    ]))
    chrome = chrome_doc(
        chrome_el("html > body", 0, 0, 400, 120),
        chrome_el("#wrapper", 0, 0, 400, 120, tag="div", id="wrapper"),
        chrome_el("body > div:nth-of-type(1) > p", 0, 0, 400, 120),
    )
    s = ed.compare(ours, chrome)["summary"]
    assert s["first_over_threshold_non_ancestor"]["path"] == "body > div:nth-of-type(1) > p"


def test_first_non_ancestor_counts_id_descendant():
    # #leaf is the culprit; its plain-path parent must not be reported.
    ours = ours_doc(box("html > body", 0, 0, 400, 100, children=[
        box("body > div.card", 0, 0, 400, 100, children=[box("#leaf", 0, 0, 400, 100, tag="p")]),
    ]))
    chrome = chrome_doc(
        chrome_el("html > body", 0, 0, 400, 120),
        chrome_el("body > div.card", 0, 0, 400, 120),
        chrome_el("#leaf", 0, 0, 400, 120, tag="p", id="leaf"),
    )
    s = ed.compare(ours, chrome)["summary"]
    assert s["first_over_threshold_non_ancestor"]["path"] == "#leaf"


def test_first_non_ancestor_ignores_unrelated_later_elements():
    # main is wrong with a clean subtree; a later unrelated aside is also wrong.
    ours = ours_doc(box("html > body", 0, 0, 400, 300, children=[
        box("body > main", 0, 10, 400, 100, children=[box("body > main > p", 0, 10, 400, 20)]),
        box("body > aside", 0, 200, 400, 50),
    ]))
    chrome = chrome_doc(
        chrome_el("html > body", 0, 0, 400, 300),
        chrome_el("body > main", 0, 0, 400, 110),
        chrome_el("body > main > p", 0, 10, 400, 20),
        chrome_el("body > aside", 0, 210, 400, 50),
    )
    s = ed.compare(ours, chrome)["summary"]
    assert s["first_over_threshold_non_ancestor"]["path"] == "body > main"


def test_svg_shapes_are_not_joined():
    svg = box("#icon", 0, 0, 24, 24, tag="svg", type="image", children=[
        {"type": "svg_shape", "tag": "rect", "selector": "#icon > rect",
         "border_box": {"x": 0, "y": 0, "width": 24, "height": 24}},
    ])
    ours, skipped = ed.load_ours(ours_doc(svg))
    assert [o["selector"] for o in ours] == ["#icon"] and skipped == 1
    assert ed._id_key({"selector": "#icon > rect"}) is None


def test_worst_is_ranked_by_area_and_capped():
    ours_children, chrome_els = [], []
    for i in range(25):
        sel = f"body > div:nth-of-type({i + 1})"
        ours_children.append(box(sel, 0, i * 10, 10 + i, 10))
        chrome_els.append(chrome_el(sel, 0, i * 10, 10, 10))
    s = ed.compare(ours_doc(*ours_children), chrome_doc(*chrome_els))["summary"]
    worst = s["worst"]
    assert len(worst) == 20  # element 0 is exact and excluded; capped at 20
    areas = [w["area_error"] for w in worst]
    assert areas == sorted(areas, reverse=True)
    assert worst[0]["path"] == "body > div:nth-of-type(25)"


def test_empty_match_has_null_share():
    s = ed.compare(ours_doc(), chrome_doc())["summary"]
    assert s["matched"] == 0 and s["within_tolerance_share"] is None
    assert s["first_over_threshold"] is None


def test_stub_text_backend_warns():
    ours, chrome = build_page()
    ours["text_metrics_font_derived"] = False
    assert "warning" in ed.compare(ours, chrome)["summary"]
    ours["text_metrics_font_derived"] = True
    assert "warning" not in ed.compare(ours, chrome)["summary"]


# --- CLI and overlay --------------------------------------------------------


def test_cli_writes_json_and_overlay(tmp_path):
    pytest.importorskip("PIL")
    from PIL import Image

    ours, chrome = build_page()
    o, c = tmp_path / "ours.json", tmp_path / "chrome.json"
    o.write_text(json.dumps(ours))
    c.write_text(json.dumps(chrome))
    frame = tmp_path / "frame.ppm"
    Image.new("RGB", (400, 300), (255, 255, 255)).save(frame)
    out, overlay = tmp_path / "diff.json", tmp_path / "overlay.png"
    rc = ed.main([str(o), str(c), "--frame", str(frame), "--overlay", str(overlay),
                  "--out", str(out), "--only-worst", "1"])
    assert rc == 0
    result = json.loads(out.read_text())
    assert result["summary"]["matched"] == 5
    assert result["unmatched"]["chrome_only"][0]["path"] == "body > aside"
    img = Image.open(overlay)
    assert img.size == (400, 300)
    # main is the worst; its Chrome box (blue) top edge is at y=40.
    assert img.getpixel((200, 40))[2] > 150 and img.getpixel((200, 40))[0] < 100


def test_overlay_without_frame_uses_viewport(tmp_path):
    pytest.importorskip("PIL")
    from PIL import Image

    ours, chrome = build_page()
    result = ed.compare(ours, chrome)
    out = tmp_path / "o.png"
    drawn = ed.draw_overlay(result, str(out), None, (400, 300), only_worst=None)
    assert drawn == 2
    assert Image.open(out).size == (400, 300)
