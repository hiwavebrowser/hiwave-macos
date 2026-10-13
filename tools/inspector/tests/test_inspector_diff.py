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
    # The cards' text lengths (10, 11) are within the tolerance, so which one
    # is missing could not be told; this card is made distinguishable.
    chrome = copy.deepcopy(CHROME)
    rec(chrome, CARD2)["tree"]["text_length"] = 40
    doc = copy.deepcopy(chrome)
    doc["elements"] = [e for e in doc["elements"] if e["identity"]["selector"] != CARD2]
    rec(doc, SECTION)["tree"]["child_count"] = 1
    r = idf.compare(doc, chrome)
    # The section's child count differs first in document order; the missing
    # card is where it enters.
    assert r["first"]["stage"] == "DOM"
    assert r["first"]["selector"] == SECTION
    assert r["first_innermost"]["selector"] == CARD2
    assert r["first_innermost"]["fields"] == [{"field": "element", "ours": "missing", "chrome": "present"}]
    assert r["summary"]["stage_counts"]["DOM"] == 2


def test_an_extra_element_is_a_dom_root():
    doc = ours()
    extra = copy.deepcopy(rec(doc, CARD2))
    extra["identity"].update(selector=f"{SECTION} > div:nth-child(3)",
                             oracle_key="body > section > div.card:nth-of-type(3)")
    extra["tree"]["text_length"] = 40  # distinguishable from the card it copies
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


def test_same_stage_children_stay_roots_and_innermost_points_at_the_deepest():
    doc = ours()
    for sel in (BODY, SECTION, CARD2):
        rec(doc, sel)["box"]["border_box"]["h"] += 12
    r = run(doc)
    assert r["first"]["selector"] == "html > body:nth-child(2)"
    assert r["first_innermost"]["selector"] == CARD2
    assert r["summary"]["stage_counts"]["GEOMETRY"] == 3


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
    assert set(r["summary"]) == {"divergent_elements", "roots", "stage_counts", "style_properties", "geometry_contexts"}
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


# ---------------------------------------------------------------------------
# R1 review inputs
# ---------------------------------------------------------------------------

CELL1 = f"{ROW} > div:nth-child(1)"
CELL3 = f"{ROW} > div:nth-child(3)"
CARD1 = f"{SECTION} > div:nth-child(1)"
GRID = f"{BODY} > div:nth-child(3)"


def both():
    return copy.deepcopy(CHROME), copy.deepcopy(CHROME)


def shift(doc, selector, **d):
    """Move/resize a record's border, padding and content boxes together."""
    for f in ("border_box", "padding_box", "content_box"):
        r = rec(doc, selector)["box"].get(f)
        if r:
            for k, v in d.items():
                r[k] = round(r[k] + v, 2)


def descendants(doc, selector):
    return [e["identity"]["selector"] for e in doc["elements"]
            if e["identity"]["selector"].startswith(selector + " > ")]


@pytest.mark.parametrize("selector,prop,ours_v,chrome_v", [
    (GRID, "grid-template-rows", "none", "200px 200px"),       # implicit tracks
    (ROW, "gap", "0px 5px", "normal 5px"),
    (ROW, "column-gap", "0px", "normal"),
    (ABS, "z-index", "auto", "0"),
    (HERO, "transform", "none", "matrix(1, 0, 0, 1, 0, 0)"),
    (HERO, "transform", "matrix(0.707, 0.707, -0.707, 0.707, 0, 0)",
     "matrix3d(0.707107, 0.707107, 0, 0, -0.707107, 0.707107, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1)"),
    (ROW, "justify-content", "flex-end", "end"),
    (ROW, "align-items", "flex-start", "start"),
    (HERO, "background-image", 'url("img/x.png")', 'url("https://site.test/a/img/x.png")'),
    (HERO, "background-image", 'url("./img/x.png")', 'url("https://site.test/a/img/x.png")'),
    (HERO, "color", "#000", "rgb(0, 0, 0)"),
    (HERO, "background-color", "rgba(255, 0, 0, 0)", "rgba(0, 0, 0, 0)"),
    (HERO, "text-align", "-webkit-center", "center"),
])
def test_serialisation_differences_are_not_style_roots(selector, prop, ours_v, chrome_v):
    o, c = both()
    rec(o, selector)["style"][prop] = ours_v
    rec(c, selector)["style"][prop] = chrome_v
    r = idf.compare(o, c)
    assert r["first"] is None, r["first"]


@pytest.mark.parametrize("selector,prop,ours_v,chrome_v", [
    (ROW, "justify-content", "flex-start", "end"),
    (ROW, "gap", "0px 6px", "normal 5px"),
    (HERO, "transform", "none", "matrix(1, 0, 0, 1, 5, 0)"),
    (HERO, "background-image", 'url("img/y.png")', 'url("https://site.test/a/img/x.png")'),
])
def test_real_differences_next_to_those_still_diverge(selector, prop, ours_v, chrome_v):
    o, c = both()
    rec(o, selector)["style"][prop] = ours_v
    rec(c, selector)["style"][prop] = chrome_v
    r = idf.compare(o, c)
    assert (r["first"]["stage"], r["first"]["selector"]) == ("STYLE", selector)


def test_a_narrower_container_is_one_root_not_one_per_child():
    doc = ours()
    shift(doc, ROW, w=-100)
    for i, sel in enumerate((CELL1, CELL2, CELL3)):
        shift(doc, sel, x=-33.33 * i, w=-33.33)
    r = run(doc)
    assert r["summary"]["stage_counts"] == {"DOM": 0, "STYLE": 0, "GEOMETRY": 1}
    assert r["first"]["selector"] == ROW
    assert r["first_innermost"]["selector"] == ROW
    assert r["summary"]["divergent_elements"] == 4


def test_a_body_padding_change_is_one_root():
    doc = ours()
    body = rec(doc, BODY)["box"]
    body["border_box"]["h"] += 10
    body["padding_box"]["h"] += 10
    body["content_box"]["y"] += 10
    rec(doc, "html")["box"]["border_box"]["h"] += 10
    rec(doc, "html")["box"]["padding_box"]["h"] += 10
    rec(doc, "html")["box"]["content_box"]["h"] += 10
    for sel in descendants(doc, BODY):
        shift(doc, sel, y=10)
    r = run(doc)
    assert r["summary"]["stage_counts"] == {"DOM": 0, "STYLE": 0, "GEOMETRY": 1}
    assert r["first"]["selector"] == BODY == r["first_innermost"]["selector"]


def test_a_taller_child_is_the_root_not_its_ancestors_or_later_siblings():
    doc = ours()
    shift(doc, CARD1, h=12)
    shift(doc, CARD2, y=12)
    for sel in (SECTION, BODY, "html"):
        for f in ("border_box", "padding_box", "content_box"):
            rec(doc, sel)["box"][f]["h"] += 12
    r = run(doc)
    assert r["summary"]["stage_counts"]["GEOMETRY"] == 1
    assert r["first"]["selector"] == CARD1 == r["first_innermost"]["selector"]


def test_an_inherited_property_counts_once_at_the_element_that_sets_it():
    o, c = both()
    for doc, colour in ((o, "rgb(255, 0, 0)"), (c, "rgb(0, 0, 0)")):
        for sel in (SECTION, CARD1, CARD2):
            rec(doc, sel)["style"]["color"] = colour
    r = idf.compare(o, c)
    assert r["summary"]["stage_counts"] == {"DOM": 0, "STYLE": 1, "GEOMETRY": 0}
    assert r["first"]["selector"] == SECTION == r["first_innermost"]["selector"]


def _drop_first_cell(doc):
    """Our side without the row's first cell, keyed as our engine would key it."""
    doc["elements"] = [e for e in doc["elements"] if e["identity"]["selector"] != CELL1]
    rec(doc, ROW)["tree"]["child_count"] = 2
    for n, sel in ((1, CELL2), (2, CELL3)):
        i = rec(doc, sel)["identity"]
        i["selector"] = f"{ROW} > div:nth-child({n})"
        i["oracle_key"] = f"body > div:nth-of-type(2) > div.cell:nth-of-type({n})"
        i["child_index"] = n - 1


def test_removing_one_of_identical_siblings_never_names_the_wrong_one():
    doc = ours()
    _drop_first_cell(doc)
    r = run(doc)
    named = [d["selector"] for d in r["roots"]]
    assert CELL3 not in named and CELL2 not in named, named
    assert (r["first_innermost"]["stage"], r["first_innermost"]["selector"]) == ("DOM", ROW)
    assert r["matching"]["unverified"] >= 1


def test_removing_one_of_distinct_siblings_names_the_removed_one():
    o, c = both()
    for doc in (o, c):
        for n, sel in enumerate((CELL1, CELL2, CELL3), 1):
            rec(doc, sel)["tree"]["text_length"] = n * 4
    _drop_first_cell(o)
    r = idf.compare(o, c)
    missing = [d for d in r["roots"] if d["fields"][0].get("ours") == "missing"]
    assert [d["selector"] for d in missing] == [CELL1]
    assert r["first_innermost"]["selector"] == CELL1
    assert r["summary"]["stage_counts"]["DOM"] == 2  # the row's child count and the cell


def test_style_and_script_text_is_not_a_dom_difference_of_their_ancestors():
    doc = ours()
    rec(doc, "html > head:nth-child(1) > style:nth-child(3)")["tree"]["text_length"] += 40
    rec(doc, "html > head:nth-child(1)")["tree"]["text_length"] += 40
    rec(doc, "html")["tree"]["text_length"] += 40
    assert run(doc)["first"] is None


def test_properties_with_no_effect_for_the_box_are_ignored():
    o, c = both()
    rec(o, CELL2)["style"]["float"] = "none"       # a flex item does not float
    rec(c, CELL2)["style"]["float"] = "left"
    rec(o, CELL2)["style"]["clear"] = "both"
    rec(o, HERO)["style"]["vertical-align"] = "middle"   # a block is not inline-level
    rec(o, HERO)["style"]["object-fit"] = "cover"        # not a replaced element
    assert idf.compare(o, c)["first"] is None


def test_broken_pipe_is_not_a_traceback(tmp_path):
    import subprocess
    # Big enough to overflow a pipe buffer: 3000 duplicated keys reported as ambiguous.
    doc = ours()
    card = rec(doc, CARD2)
    for i in range(3000):
        for _ in range(2):
            e = copy.deepcopy(card)
            e["identity"]["oracle_key"] = f"body > section > div.k{i}"
            doc["elements"].append(e)
    o = tmp_path / "o.json"
    o.write_text(json.dumps(doc))
    p = subprocess.run(
        f"{sys.executable} {HERE.parent / 'inspector_diff.py'} {o} {o} | head -c 10",
        shell=True, capture_output=True, text=True)
    assert "BrokenPipeError" not in p.stderr and "Traceback" not in p.stderr, p.stderr


SCRUB_CASES = [
    # (value, page url, expected)
    ('image-set(url("img.png?token=IMGSET") 1x)', "https://s.test/p", 'image-set(url("img.png") 1x)'),
    ('image-set(url("data:image/png;base64,AAAA") 1x)', "https://s.test/p", 'image-set(url("data:…") 1x)'),
    ('url("file:///home/someone/repo/case/img/a.png")', "file:///home/someone/repo/case/index.html", 'url("img/a.png")'),
    ('url("file:///home/someone/other.png")', "file:///home/someone/repo/case/index.html", 'url("../../other.png")'),
    ('url("https://u:p@cdn.test/i.png?sig=T#f")', "https://s.test/p", 'url("https://cdn.test/i.png")'),
    ('-webkit-linear-gradient(top, red, blue)', "https://s.test/p", "linear-gradient(…)"),
    ('url("a.png"), radial-gradient(red, blue)', "https://s.test/p", 'url("a.png"), radial-gradient(…)'),
]


def test_chrome_background_image_scrubbing():
    import shutil
    import subprocess
    if not shutil.which("node"):
        pytest.skip("node not installed")
    mod = (HERE.parent / "record_scrub.mjs").as_uri()
    script = (f"const m = await import({json.dumps(mod)});"
              f"const cases = {json.dumps(SCRUB_CASES)};"
              "console.log(JSON.stringify(cases.map(([v, u]) => m.backgroundImage(v, u))));")
    p = subprocess.run(["node", "--input-type=module", "-e", script], capture_output=True, text=True)
    assert p.returncode == 0, p.stderr
    assert json.loads(p.stdout) == [e for _, _, e in SCRUB_CASES]


def test_an_own_padding_change_under_a_moved_parent_stays_a_root():
    doc = ours()
    shift(doc, SECTION, h=5)
    rec(doc, CARD2)["box"]["content_box"]["x"] += 5  # padding-left, border box unchanged
    rec(doc, CARD2)["box"]["content_box"]["w"] -= 5
    r = run(doc)
    assert CARD2 in [d["selector"] for d in r["roots"]]


def test_min_width_auto_is_0px_except_on_flex_and_grid_items():
    o, c = both()
    rec(o, HERO)["style"]["min-width"] = "auto"
    rec(c, HERO)["style"]["min-width"] = "0px"
    assert idf.compare(o, c)["first"] is None
    rec(o, CELL2)["style"]["min-width"] = "0px"   # a flex item: auto and 0 differ
    rec(c, CELL2)["style"]["min-width"] = "auto"
    r = idf.compare(o, c)
    assert (r["first"]["stage"], r["first"]["selector"]) == ("STYLE", CELL2)


def _chrome_records(tmp_path, target):
    """Run chrome_records.mjs on `target`; skip where Chromium cannot launch."""
    import shutil
    import subprocess
    if not shutil.which("node"):
        pytest.skip("node not installed")
    out = tmp_path / "out.json"
    p = subprocess.run(["node", str(HERE.parent / "chrome_records.mjs"), target, str(out)],
                       capture_output=True, text=True, timeout=120)
    if "Executable doesn't exist" in p.stderr or "PARITY_CHROME_PATH set but not found" in p.stderr:
        pytest.skip("Chromium not installed (set PARITY_CHROME_PATH)")
    return p, out


FAILED_NAV = "https://nonexistent.invalid/p?token=SECRET#frag"


def test_a_failed_navigation_keeps_no_query_or_fragment(tmp_path):
    p, out = _chrome_records(tmp_path, FAILED_NAV)
    printed = p.stdout + p.stderr + (out.read_text() if out.exists() else "")
    assert "SECRET" not in printed and "#frag" not in printed, printed


def test_a_navigation_error_names_the_target_as_the_record_does():
    import shutil
    import subprocess
    if not shutil.which("node"):
        pytest.skip("node not installed")
    mod = (HERE.parent / "record_scrub.mjs").as_uri()
    cases = [
        ["page.goto: net::ERR_NAME_NOT_RESOLVED at https://h.test/p?token=SECRET#frag",
         "https://h.test/p?token=SECRET#frag", "https://h.test/p"],
        # A URL the message spells differently (normalised) is still scrubbed.
        ["page.goto: Timeout 30000ms exceeded.\nnavigating to \"https://h.test/a%20b?sig=SECRET\"",
         "https://h.test/a b?sig=SECRET", "https://h.test/a b"],
        ["page.goto: net::ERR_FILE_NOT_FOUND at file:///home/someone/case.html",
         "file:///home/someone/case.html", "case.html"],
        ["page.goto: net::ERR_ABORTED at data:text/html,<p>SECRET</p>",
         "data:text/html,<p>SECRET</p>", "data:text/html"],
    ]
    script = (f"const m = await import({json.dumps(mod)});"
              f"const cases = {json.dumps(cases)};"
              "console.log(JSON.stringify(cases.map(([msg, t, n]) => m.redactTarget(msg, [t], n))));")
    p = subprocess.run(["node", "--input-type=module", "-e", script], capture_output=True, text=True)
    assert p.returncode == 0, p.stderr
    got = json.loads(p.stdout)
    for g, (_, _, named) in zip(got, cases):
        assert "SECRET" not in g and "#frag" not in g and "/home/" not in g, g
        assert named in g, g


def test_a_failed_navigation_is_an_error_with_no_records(tmp_path):
    p, out = _chrome_records(tmp_path, FAILED_NAV)
    result = json.loads(p.stdout.strip().splitlines()[-1])
    assert p.returncode != 0, p.stdout
    assert result["status"] == "error", result
    assert "net::ERR_" in result["error"], result
    # Chromium's error page is not the page asked for: nothing is recorded.
    assert not out.exists() and "records_path" not in result, result


@pytest.mark.parametrize("target", ["data:text/html,<p>x</p>", "about:blank"])
def test_a_navigation_with_no_network_response_is_still_ok(tmp_path, target):
    p, out = _chrome_records(tmp_path, target)
    assert p.returncode == 0, p.stdout + p.stderr
    assert json.loads(p.stdout.strip().splitlines()[-1])["status"] == "ok"
    assert out.exists()


# Lead request after #674: `;` path parameters (a servlet `;jsessionid=` is
# the session cookie) go with the query, in every segment, `%3B` too; a
# `javascript:` URL is all payload and a `blob:` URL keeps only its origin.
# Each case mirrors one in #674's diagnostics_tests.rs; the expected string
# is the engine's, except where noted (the engine keeps a URL's spelling,
# Chromium gives the parser's).

def _node(expr, cases):
    """`expr` (a function of m, the module, and one case) over `cases`, in node."""
    import shutil
    import subprocess
    if not shutil.which("node"):
        pytest.skip("node not installed")
    mod = (HERE.parent / "record_scrub.mjs").as_uri()
    script = (f"const m = await import({json.dumps(mod)});"
              f"const cases = {json.dumps(cases)};"
              f"console.log(JSON.stringify(cases.map((c) => ({expr})(c))));")
    p = subprocess.run(["node", "--input-type=module", "-e", script], capture_output=True, text=True)
    assert p.returncode == 0, p.stderr
    return json.loads(p.stdout)


SCRUB_URL_CASES = [
    # (in, out); engine scrub_url gives the same unless noted.
    ("https://cdn.test/a.png;jsessionid=0123ABCD", "https://cdn.test/a.png"),
    ("https://cdn.test/a.png;JSESSIONID=S?x", "https://cdn.test/a.png"),
    ("https://cdn.test/a;jsessionid=S/b.png", "https://cdn.test/a/b.png"),
    ("https://cdn.test/a;x=1;jsessionid=S/b;v=2/c.png#f", "https://cdn.test/a/b/c.png"),
    ("https://cdn.test/a%3Bjsessionid=S/b.png", "https://cdn.test/a/b.png"),
    ("https://cdn.test/a.png%3bjsessionid=S", "https://cdn.test/a.png"),
    ("https://cdn.test/a.png;", "https://cdn.test/a.png"),
    # Engine: "https:\\\\cdn.test\\a\\b.png" (its spelling, no parameter).
    ("https:\\\\cdn.test\\a;jsessionid=S\\b.png", "https://cdn.test/a/b.png"),
    ("https://cdn.test/a.png;jsess\tion\rid=S", "https://cdn.test/a.png"),
    ("//cdn.test/a;jsessionid=S/b.png", "//cdn.test/a/b.png"),
    ("img/a;jsessionid=S/b.png", "img/a/b.png"),
    ("/img/a.png;jsessionid=S", "/img/a.png"),
    ("a.png%3Bjsessionid=S", "a.png"),
    ("img\\a;jsessionid=S\\b.png", "img\\a\\b.png"),
    ("data:image/png;base64,AAAA", "data:…"),
    ("blob:https://cdn.test/2b8c;jsessionid=S", "blob:https://cdn.test"),
    ("https://u:p@cdn.test/a;jsessionid=S/b.png?t=1", "https://cdn.test/a/b.png"),
    ("//u:p@cdn.test/i.png", "//cdn.test/i.png"),
    ("img/a@2x.png", "img/a@2x.png"),
    # Engine: "https:cdn.test/a.png" (its spelling).
    ("https:u:p@cdn.test/a.png;jsessionid=S", "https://cdn.test/a.png"),
    ("javascript:alert(document.cookie)", "javascript:"),
    ("JavaScript:alert(1)", "javascript:"),
    ("java\tscript:alert(document.cookie)", "javascript:"),
    (" \tjavascript:alert(1)", "javascript:"),
    ("blob:https://u:PW@cdn.test/2b8c-uuid", "blob:https://cdn.test"),
    ("blob:2b8c-uuid", "blob:"),
    ("DATA:text/plain,SECRET", "data:…"),
    (" \tdata:text/plain,SECRET", "data:…"),
    ("da\tta:text/plain,SECRET", "data:…"),
    ("\u0001data:text/plain,SECRET", "data:…"),
    ("data\r\n:text/plain,SECRET", "data:…"),
    ("http://shop.test:8080/a;jsessionid=S#f", "http://shop.test:8080/a"),
    ("https://shop.test/;jsessionid=S", "https://shop.test/"),
    ("foo://shop.test/a;sid=S/b", "foo://shop.test/a/b"),
    ("mailto:a;sid=S", "mailto:a"),
]


@pytest.mark.parametrize("u,want", SCRUB_URL_CASES)
def test_scrub_url_drops_path_parameters_and_script_and_blob_payloads(u, want):
    assert _node("([u]) => m.scrubUrl(u, 'https://s.test/p')", [[u]]) == [want]


REPORT_TARGET_CASES = [
    # (target, default, with --keep-query); engine report_url gives the same.
    ("https://example.test/a/b?token=1#frag", "https://example.test/a/b", "https://example.test/a/b?token=1#frag"),
    ("https://shop.test/cart;jsessionid=0123ABCD?x=1", "https://shop.test/cart",
     "https://shop.test/cart;jsessionid=0123ABCD?x=1"),
    ("https://u:p@shop.test/cart;jsessionid=0123ABCD?x=1", "https://shop.test/cart",
     "https://shop.test/cart;jsessionid=0123ABCD?x=1"),
    ("https://shop.test/a;jsessionid=S/b;v=1/c.png", "https://shop.test/a/b/c.png",
     "https://shop.test/a;jsessionid=S/b;v=1/c.png"),
    ("https://shop.test/a;x=1;JSESSIONID=S", "https://shop.test/a", "https://shop.test/a;x=1;JSESSIONID=S"),
    ("https://shop.test/a%3Bjsessionid=S/b", "https://shop.test/a/b", "https://shop.test/a%3Bjsessionid=S/b"),
    ("https://shop.test/a%3bjsessionid=S/b", "https://shop.test/a/b", "https://shop.test/a%3bjsessionid=S/b"),
    ("https://shop.test/a;", "https://shop.test/a", "https://shop.test/a;"),
    ("https://shop.test/;jsessionid=S", "https://shop.test/", "https://shop.test/;jsessionid=S"),
    ("https:\\\\shop.test\\a;jsessionid=S\\b", "https://shop.test/a/b", "https://shop.test/a;jsessionid=S/b"),
    ("https://shop.test/a;jsess\tionid=S\n/b", "https://shop.test/a/b", "https://shop.test/a;jsessionid=S/b"),
    ("http://shop.test:8080/a;jsessionid=S#f", "http://shop.test:8080/a", "http://shop.test:8080/a;jsessionid=S#f"),
    ("foo://shop.test/a;sid=S/b", "foo://shop.test/a/b", "foo://shop.test/a;sid=S/b"),
    # data:, javascript: and blob: keep their reduction with or without the opt-in.
    ("data:text/html;charset=utf-8,x;y", "data:text/html", "data:text/html"),
    ("data:text/html,<p>secret-body</p>?q=1", "data:text/html", "data:text/html"),
    ("data:text/html;charset=utf-8;base64,PHA+c2VjcmV0PC9wPg==", "data:text/html", "data:text/html"),
    ("DATA:,secret-body", "data:", "data:"),
    ("data:SECRETNOCOMMA", "data:", "data:"),
    ("blob:https://shop.test/a;jsessionid=S", "blob:https://shop.test", "blob:https://shop.test"),
    ("blob:https://user:PW@example.test/2b8c-uuid", "blob:https://example.test", "blob:https://example.test"),
    ("blob:2b8c-uuid", "blob:", "blob:"),
    ("javascript:alert(document.cookie)", "javascript:", "javascript:"),
    ("about:blank", "about:blank", "about:blank"),
]


@pytest.mark.parametrize("target,want,want_kept", REPORT_TARGET_CASES)
def test_the_page_url_drops_path_parameters_unless_asked(target, want, want_kept):
    got = _node("([t]) => [m.isUrlTarget(t), m.reportTarget(t, m.isUrlTarget(t), false, '/w'),"
                " m.reportTarget(t, m.isUrlTarget(t), true, '/w')]", [[target]])
    assert got == [[True, want, want_kept]]


BG_PARAM_CASES = [
    # getComputedStyle's value for #674's in-page reproducers, and the record.
    ('url("https://cdn.test/a.png;jsessionid=0123ABCD")', "https://s.test/p", 'url("https://cdn.test/a.png")'),
    ('url("https://cdn.test/a;jsessionid=0123ABCD/b.png%3Bv=0123ABCD")', "https://s.test/p",
     'url("https://cdn.test/a/b.png")'),
    ('url("javascript:alert(document.cookie)")', "https://s.test/p", 'url("javascript:")'),
    ('url("blob:https://s.test/2b8c-uuid")', "https://s.test/p", 'url("blob:https://s.test")'),
    ('url("file:///home/someone/repo/case/img/a.png;jsessionid=0123ABCD")',
     "file:///home/someone/repo/case/index.html", 'url("img/a.png")'),
]


def test_background_urls_drop_path_parameters_and_payloads():
    assert _node("([v, u]) => m.backgroundImage(v, u)", BG_PARAM_CASES) == [e for _, _, e in BG_PARAM_CASES]


def test_messages_drop_path_parameters_and_script_and_blob_payloads():
    target = "https://h.test/cart;jsessionid=0123ABCD?x=1"
    msgs = [
        f"page.goto: net::ERR_NAME_NOT_RESOLVED at {target}",
        "net::ERR_FAILED at https://other.test/a;jsessionid=0123ABCD/b%3Bv=0123ABCD",
        "Error: blocked javascript:fetch('/x?c='+document.cookie+'0123ABCD') here",
        "Error: blob:https://u:PW@h.test/0123ABCD not allowed",
    ]
    got = _node(f"([msg]) => m.redactTarget(msg, [{json.dumps(target)}],"
                f" m.reportTarget({json.dumps(target)}, true, false, '/w'))", [[x] for x in msgs])
    for g in got:
        assert "0123ABCD" not in g and "cookie" not in g and "PW" not in g, g
    assert "https://h.test/cart" in got[0] and "https://other.test/a/b" in got[1], got
    assert "javascript:" in got[2] and "blob:https://h.test" in got[3], got


def test_a_session_id_in_the_target_or_a_background_never_reaches_the_output(tmp_path):
    p, out = _chrome_records(tmp_path, "https://nonexistent.invalid/p;jsessionid=0123ABCD/q%3Bv=0123ABCD?t=1")
    printed = p.stdout + p.stderr + (out.read_text() if out.exists() else "")
    assert "0123ABCD" not in printed, printed
    assert json.loads(p.stdout.strip().splitlines()[-1])["target"] == "https://nonexistent.invalid/p/q"
    page = ("data:text/html,<style>%23d{width:10px;height:10px;"
            "background-image:url('https://cdn.invalid/a;jsessionid=0123ABCD/b.png%3Bv=0123ABCD'),"
            "url('javascript:alert(0123ABCD)')}</style><div id=d></div>")
    p, out = _chrome_records(tmp_path, page)
    assert p.returncode == 0, p.stdout + p.stderr
    printed = p.stdout + p.stderr + out.read_text()
    assert "0123ABCD" not in printed, printed
    d = next(e for e in json.loads(out.read_text())["elements"] if e["identity"].get("id") == "d")
    assert d["style"]["background-image"] == 'url("https://cdn.invalid/a/b.png"), url("javascript:")'
