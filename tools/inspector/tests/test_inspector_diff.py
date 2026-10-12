"""inspector_diff: one planted difference of each kind is named, with its stage.

The Chrome side is a real record of tests/pages/planted.html
(fixtures/planted.chrome.json, written by chrome_records.mjs). The "ours" side
starts as a copy of it, so the two agree, and each test edits the copy.
"""

import copy
import json
import sys
from pathlib import Path

import pytest

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))

import inspector_diff as idf  # noqa: E402

CHROME = json.loads((HERE / "fixtures" / "planted.chrome.json").read_text())

BODY = "html > body:nth-child(2)"
HERO = f"{BODY} > div:nth-child(1)"
ROW = f"{BODY} > div:nth-child(2)"
CELL2 = f"{ROW} > div:nth-child(2)"
GRID_P2 = f"{BODY} > div:nth-child(3) > p:nth-child(2)"
ABS = f"{BODY} > div:nth-child(4) > span"
SECTION = f"{BODY} > section:nth-child(5)"
CARD2 = f"{SECTION} > div:nth-child(2)"


def ours():
    return copy.deepcopy(CHROME)


def rec(doc, selector):
    for e in doc["elements"]:
        if e["identity"]["selector"] == selector:
            return e
    raise KeyError(selector)


def run(doc, **kw):
    return idf.compare(doc, CHROME, **kw)


def test_identical_records_have_no_divergence():
    r = run(ours())
    assert r["first"] is None
    assert r["summary"]["stage_counts"] == {"DOM": 0, "STYLE": 0, "GEOMETRY": 0}
    assert r["matching"]["matched"] == len(CHROME["elements"])
    assert r["matching"]["ambiguous"] == []


def test_a_missing_element_is_a_dom_root():
    doc = ours()
    doc["elements"] = [e for e in doc["elements"] if e["identity"]["selector"] != CARD2]
    rec(doc, SECTION)["tree"]["child_count"] = 1
    r = run(doc)
    # The section's child count is what the missing card makes of it: the
    # card is the root, the section a consequence.
    assert r["first"]["stage"] == "DOM"
    assert r["first"]["selector"] == CARD2
    assert r["first"]["fields"] == [{"field": "element", "ours": "missing", "chrome": "present"}]
    assert r["summary"]["stage_counts"]["DOM"] == 1
    assert r["summary"]["consequences"] >= 1


def test_an_extra_element_is_a_dom_root():
    doc = ours()
    extra = copy.deepcopy(rec(doc, CARD2))
    extra["identity"].update(selector=f"{SECTION} > div:nth-child(3)",
                             oracle_key="body > section > div.card:nth-of-type(3)")
    doc["elements"].append(extra)
    r = run(doc)
    assert r["matching"]["ours_only"] == 1
    roots = [d for d in r["roots"] if d["fields"][0].get("chrome") == "missing"]
    assert [d["selector"] for d in roots] == [f"{SECTION} > div:nth-child(3)"]
    assert roots[0]["stage"] == "DOM"


def test_a_different_tag_and_text_length_are_dom():
    doc = ours()
    rec(doc, GRID_P2)["identity"]["tag"] = "div"
    r = run(doc)
    assert r["first"]["selector"] == GRID_P2
    assert r["first"]["fields"][0] == {"field": "tag", "ours": "div", "chrome": "p"}

    doc = ours()
    rec(doc, HERO)["tree"]["text_length"] += 2  # within the tolerance
    assert run(doc)["first"] is None
    rec(doc, HERO)["tree"]["text_length"] += 5
    r = run(doc)
    assert (r["first"]["stage"], r["first"]["selector"]) == ("DOM", HERO)


def test_a_cascade_difference_is_style_and_names_the_property():
    doc = ours()
    rec(doc, CARD2)["style"]["color"] = "rgb(255, 0, 0)"
    r = run(doc)
    assert r["first"]["stage"] == "STYLE"
    assert r["first"]["selector"] == CARD2
    assert r["first"]["fields"] == [{"property": "color", "ours": "rgb(255, 0, 0)", "chrome": "rgb(20, 20, 20)"}]
    assert r["summary"]["style_properties"] == {"color": 1}


def test_equivalent_serialisations_are_not_style_differences():
    doc = ours()
    hero = rec(doc, HERO)
    hero["style"]["background-color"] = "rgba(200, 220, 255, 1)"
    hero["style"]["border-top-width"] = "0"
    hero["style"]["font-family"] = "'Arial', sans-serif"
    hero["style"]["text-align"] = "left"
    hero["style"]["font-weight"] = "normal"
    row = rec(doc, ROW)
    row["style"]["justify-content"] = "flex-start"
    row["style"]["align-items"] = "stretch"
    # Unknown on our side is not a difference.
    rec(doc, CELL2)["style"]["flex-grow"] = None
    assert run(doc)["first"] is None


def test_properties_that_cannot_matter_for_the_display_type_are_ignored():
    doc = ours()
    # flex-direction on a block, flex-grow on a non-flex-item, z-index on a
    # static box, grid lines outside a grid: none of them can change layout.
    rec(doc, HERO)["style"]["flex-direction"] = "column"
    rec(doc, CARD2)["style"]["flex-grow"] = "3"
    rec(doc, CARD2)["style"]["z-index"] = "5"
    rec(doc, HERO)["style"]["grid-column-start"] = "2"
    assert run(doc)["first"] is None
    # The same properties do matter where they apply.
    rec(doc, CELL2)["style"]["flex-grow"] = "3"
    r = run(doc)
    assert (r["first"]["stage"], r["first"]["selector"]) == ("STYLE", CELL2)


def test_a_box_difference_is_geometry_with_its_deltas_and_context():
    doc = ours()
    bb = rec(doc, CELL2)["box"]["border_box"]
    bb["w"] += 10
    r = run(doc)
    assert r["first"]["stage"] == "GEOMETRY"
    assert r["first"]["selector"] == CELL2
    f = r["first"]["fields"][0]
    assert (f["field"], f["dx"], f["dy"], f["dw"], f["dh"]) == ("border_box", 0.0, 0.0, 10.0, 0.0)
    assert r["summary"]["geometry_contexts"] == {"flex item": 1}
    assert r["first"]["affected_area"] == 400.0  # 10px x 40px


def test_a_box_within_the_threshold_matches_and_the_threshold_is_a_flag():
    doc = ours()
    rec(doc, ABS)["box"]["border_box"]["x"] += 1.5
    assert run(doc)["first"] is None
    r = run(doc, threshold=1.0)
    assert (r["first"]["stage"], r["first"]["selector"]) == ("GEOMETRY", ABS)
    assert r["summary"]["geometry_contexts"] == {"abspos": 1}


def test_a_child_of_an_earlier_stage_divergence_is_not_a_root():
    doc = ours()
    rec(doc, ROW)["style"]["flex-direction"] = "column"  # STYLE on the container
    rec(doc, CELL2)["box"]["border_box"]["y"] += 40      # its consequence
    r = run(doc)
    assert r["first"]["selector"] == ROW and r["first"]["stage"] == "STYLE"
    assert [d["selector"] for d in r["roots"]] == [ROW]
    assert r["summary"]["divergent_elements"] == 2
    assert r["summary"]["stage_counts"] == {"DOM": 0, "STYLE": 1, "GEOMETRY": 0}


def test_ancestors_that_only_grow_with_a_taller_child_are_consequences():
    doc = ours()
    for sel in ("html", BODY, SECTION, CARD2):
        rec(doc, sel)["box"]["border_box"]["h"] += 12
    r = run(doc)
    assert r["first"]["selector"] == CARD2
    assert r["first_innermost"]["selector"] == CARD2
    assert r["summary"]["stage_counts"]["GEOMETRY"] == 1
    assert r["summary"]["consequences"] == 3


def test_a_duplicated_key_is_reported_ambiguous_never_first_matched():
    doc = ours()
    a = rec(doc, f"{SECTION} > div:nth-child(1)")
    b = rec(doc, CARD2)
    a["identity"]["oracle_key"] = b["identity"]["oracle_key"] = "body > section > div.card"
    a["identity"]["id"] = b["identity"]["id"] = None
    chrome = copy.deepcopy(CHROME)
    for sel in (f"{SECTION} > div:nth-child(1)", CARD2):
        rec(chrome, sel)["identity"]["oracle_key"] = "body > section > div.card"
    b["box"]["border_box"]["w"] += 50  # would be GEOMETRY if paired
    r = idf.compare(doc, chrome)
    assert r["matching"]["ambiguous"] == [{"key": "body > section > div.card", "ours_count": 2, "chrome_count": 2}]
    assert r["first"] is None


def test_head_elements_are_matched_but_classified_only_on_request():
    doc = ours()
    rec(doc, "html > head:nth-child(1) > title:nth-child(2)")["tree"]["text_length"] += 15
    assert run(doc)["first"] is None
    r = run(doc, include_head=True)
    assert r["first"]["selector"] == "html > head:nth-child(1) > title:nth-child(2)"


def test_normalize_key_reconciles_both_class_spellings():
    assert idf.normalize_key("body > div.card featured x:nth-of-type(2)") == "body > div.card.featured:nth-of-type(2)"
    assert idf.normalize_key("body > div.card.featured:nth-of-type(2)") == "body > div.card.featured:nth-of-type(2)"
    assert idf.normalize_key("#hero") == "#hero"


@pytest.mark.parametrize("prop,a,b", [
    ("color", "rgb(1, 2, 3)", "rgba(1, 2, 3, 1)"),
    ("background-color", "transparent", "rgba(0, 0, 0, 0)"),
    ("margin-top", "0", "0px"),
    ("opacity", "0.5", "0.500"),
    ("font-family", '"Helvetica Neue", Arial', "helvetica neue,arial"),
    ("gap", "normal", "0px"),
    ("aspect-ratio", "1.5 / 1", "3 / 2"),
    ("display", "list-item", "block"),
])
def test_normalize_value_equivalences(prop, a, b):
    assert idf.normalize_value(prop, a) == idf.normalize_value(prop, b)


def test_the_summary_is_stable_and_complete():
    doc = ours()
    rec(doc, CARD2)["style"]["color"] = "rgb(255, 0, 0)"
    rec(doc, CELL2)["box"]["border_box"]["w"] += 10
    a = json.dumps(run(doc), sort_keys=True)
    assert a == json.dumps(run(copy.deepcopy(doc)), sort_keys=True)
    r = json.loads(a)
    assert r["schema"] == "hiwave-inspector-diff/1"
    assert set(r) == {"schema", "page", "settings", "matching", "first", "first_innermost", "roots", "summary"}
    assert set(r["summary"]) == {"divergent_elements", "roots", "consequences", "stage_counts",
                                 "style_properties", "geometry_contexts"}
    assert r["summary"]["stage_counts"] == {"DOM": 0, "STYLE": 1, "GEOMETRY": 1}
    # Ranked by affected area: the recoloured card's whole box (800x34) over
    # the flex item's 10px-wide sliver (10x40).
    assert [(d["stage"], d["affected_area"]) for d in r["roots"]] == [("STYLE", 27200.0), ("GEOMETRY", 400.0)]


def test_report_and_cli(tmp_path, capsys):
    doc = ours()
    rec(doc, CELL2)["box"]["border_box"]["w"] += 10
    o, c, out = tmp_path / "o.json", tmp_path / "c.json", tmp_path / "d.json"
    o.write_text(json.dumps(doc))
    c.write_text(json.dumps(CHROME))
    assert idf.main([str(o), str(c), "--out", str(out)]) == 0
    text = capsys.readouterr().out
    assert f"first divergence: [GEOMETRY] {CELL2}" in text
    assert "dw +10" in text
    assert json.loads(out.read_text())["first"]["selector"] == CELL2


# --- R1 review reproductions (#675) ----------------------------------------


def shift(doc, sel, **d):
    for field in ("border_box", "padding_box", "content_box"):
        r = rec(doc, sel)["box"].get(field)
        if r:
            for k, v in d.items():
                r[k] += v


def test_children_that_follow_a_narrower_parent_are_consequences():
    doc = ours()
    # #row is 100px narrower; its three flex cells each lose a third and the
    # later ones move left by the earlier ones' loss.
    shift(doc, ROW, w=-100)
    for i, sel in enumerate(f"{ROW} > div:nth-child({n})" for n in (1, 2, 3)):
        shift(doc, sel, x=-33.33 * i, w=-33.33)
    r = run(doc)
    assert [d["selector"] for d in r["roots"]] == [ROW]
    assert r["first_innermost"]["selector"] == ROW
    assert r["summary"]["consequences"] == 3


def test_a_parent_padding_shift_is_one_root_not_every_descendant():
    doc = ours()
    body = rec(doc, BODY)["box"]
    body["border_box"]["h"] += 10
    body["padding_box"]["h"] += 10
    body["content_box"]["y"] += 10
    for e in doc["elements"]:
        sel = e["identity"]["selector"]
        if sel.startswith(BODY + " > "):
            shift(doc, sel, y=10)
    rec(doc, "html")["box"]["border_box"]["h"] += 10
    r = run(doc)
    assert r["summary"]["stage_counts"] == {"DOM": 0, "STYLE": 0, "GEOMETRY": 1}
    assert r["first"]["selector"] == BODY


def test_an_inherited_colour_is_one_style_root():
    doc = ours()
    for sel in (SECTION, f"{SECTION} > div:nth-child(1)", CARD2):
        rec(doc, sel)["style"]["color"] = "rgb(255, 0, 0)"
    chrome = copy.deepcopy(CHROME)
    for sel in (SECTION, f"{SECTION} > div:nth-child(1)", CARD2):
        rec(chrome, sel)["style"]["color"] = "rgb(20, 20, 20)"
    r = idf.compare(doc, chrome)
    assert [d["selector"] for d in r["roots"]] == [SECTION]
    assert r["summary"]["style_properties"] == {"color": 1}


def test_removing_the_first_of_unkeyed_siblings_names_it_not_the_last():
    chrome = copy.deepcopy(CHROME)
    # Three cards, no ids, distinct texts ("one", "two", "three").
    base = rec(chrome, f"{SECTION} > div:nth-child(1)")
    third = copy.deepcopy(base)
    third["identity"].update(selector=f"{SECTION} > div:nth-child(3)",
                             oracle_key="body > section > div.card:nth-of-type(3)")
    chrome["elements"].append(third)
    for n, text in ((1, 3), (2, 3), (3, 5)):
        rec(chrome, f"{SECTION} > div:nth-child({n})")["tree"]["text_length"] = text
    rec(chrome, SECTION)["tree"]["child_count"] = 3
    # Ours lost the first card: the other two moved up a position.
    doc = copy.deepcopy(chrome)
    doc["elements"] = [e for e in doc["elements"] if e["identity"]["selector"] != f"{SECTION} > div:nth-child(1)"]
    for n in (2, 3):
        e = rec(doc, f"{SECTION} > div:nth-child({n})")
        e["identity"]["selector"] = f"{SECTION} > div:nth-child({n - 1})"
        e["identity"]["oracle_key"] = f"body > section > div.card:nth-of-type({n - 1})"
    rec(doc, SECTION)["tree"]["child_count"] = 2
    r = idf.compare(doc, chrome)
    assert r["first"]["selector"] == f"{SECTION} > div:nth-child(1)"
    assert r["first"]["fields"][0]["ours"] == "missing"
    assert r["summary"]["stage_counts"]["DOM"] == 1
    assert r["matching"]["realigned"] >= 2


@pytest.mark.parametrize("prop,ours_v,chrome_v,display", [
    ("grid-template-rows", "none", "200px 200px 200px 200px", "grid"),
    ("gap", "0px 5px", "normal 5px", "flex"),
    ("z-index", "auto", "0", None),
    ("transform", "none", "matrix(1, 0, 0, 1, 0, 0)", None),
    ("transform", "matrix(1, 0, 0, 1, 5, 0)", "matrix3d(1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 5, 0, 0, 1)", None),
    ("justify-content", "flex-end", "end", "flex"),
    ("align-items", "flex-start", "start", "flex"),
    ("background-image", 'url("img/a.png")', 'url("file:///x/img/a.png")', None),
    ("background-image", "linear-gradient(…)", "-webkit-linear-gradient(…)", None),
])
def test_review_false_positives_are_not_style_roots(prop, ours_v, chrome_v, display):
    doc, chrome = ours(), copy.deepcopy(CHROME)
    target = ROW if display else HERO
    for d in (doc, chrome):
        st = rec(d, target)["style"]
        if display:
            st["display"] = display
        st["position"] = "absolute" if prop == "z-index" else st["position"]
    rec(doc, target)["style"][prop] = ours_v
    rec(chrome, target)["style"][prop] = chrome_v
    r = idf.compare(doc, chrome)
    assert r["first"] is None, r["first"]


def test_float_and_vertical_align_on_flex_items_and_object_fit_on_blocks_are_ignored():
    doc = ours()
    rec(doc, CELL2)["style"]["float"] = "left"
    rec(doc, CELL2)["style"]["vertical-align"] = "middle"
    rec(doc, HERO)["style"]["object-fit"] = "cover"
    rec(doc, HERO)["style"]["vertical-align"] = "top"
    assert run(doc)["first"] is None
