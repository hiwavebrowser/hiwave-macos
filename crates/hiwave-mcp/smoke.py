#!/usr/bin/env python3
"""Smoke test for hiwave-mcp: drive the real protocol over stdio.

Run:  cargo build -p hiwave-mcp && python3 crates/hiwave-mcp/smoke.py

This asserts the property the crate exists for — that an agent can read what
the engine COMPUTED, not just look at what it painted. The fixture is chosen
so the correct answer is derivable from the CSS by hand:

    .hero { width: 400px; height: 120px; padding: 16px; background: #08c }

with the default content-box sizing, the border box must be 432 x 152. A
pixel diff can tell you the hero looks wrong; only the layout tree can tell
you whether layout or paint got it wrong.

Both halves of that boundary are now checked. `hiwave_layout` must compute a
432x152 border box, and `hiwave_display_list` must then paint rgb(0,136,204)
over that same rectangle. Two stages agreeing on one hand-derived number is
what makes "layout is right, paint is wrong" a statement an agent can make
rather than infer.

Assertions here are deliberately restricted to values derivable from the CSS
and the layout — geometry, colour, cascade, paint order. Font-metric outputs
(baseline y, ascent, advance widths) are NOT asserted: they legitimately
differ by text stack, and pinning them would make the gate report a platform
difference as an engine regression.

`hiwave_diff` is checked in BOTH directions, because a comparison tool that
can only agree is not a comparison tool. `hero` must agree with its reference
on every hand-derived value; `important-width` must DISAGREE, on a real bug —
the cascade parses `!important` and never reads it — and must name the two
paths and both numbers. See `cases/README.md`.
"""
import json
import subprocess
import sys
from pathlib import Path

BIN = Path(__file__).resolve().parents[2] / "target" / "debug" / "hiwave-mcp"
CASES = Path(__file__).resolve().parent / "cases"

# `div { width: 100px }` sits AFTER `.hero` on purpose and is load-bearing for
# the hiwave_style assertions: it matches the same element, it is later in
# source order, and it has LOWER specificity (0,0,1 vs 0,1,0). So it must lose.
# That makes the cascade's decision visible in two independent places — the
# geometry stays 432x152 only if specificity beat source order, and
# hiwave_style has to name `.hero` as the winner and `div` as the loser.
FIXTURE = """<!DOCTYPE html><html><head><style>
body { margin: 0 }
.hero { width: 400px; height: 120px; padding: 16px; background: #08c }
div { width: 100px }
h1 { font-size: 32px; margin: 0 }
.copy { font-size: 20px; line-height: 1.5; font-family: Georgia, serif; text-align: center;
        font-style: italic }
.plain  { width: 400px; font-size: 20px; font-family: Georgia, serif }
.spaced { width: 400px; font-size: 20px; font-family: Georgia, serif; letter-spacing: 0.1em }
.pre { white-space: pre }
</style></head><body><div class="hero"><h1>Attribution</h1></div>\
<div class="copy"><span id="inherits">inherited</span></div>\
<div class="plain"><span>spacing</span></div>\
<div class="spaced"><span>spacing</span></div>\
<div class="pre">a  b<span class="nested">c  d</span></div></body></html>"""


class Client:
    def __init__(self, binary):
        self.proc = subprocess.Popen(
            [str(binary)], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.PIPE, text=True, bufsize=1,
        )
        self._id = 0

    def call(self, method, params=None):
        self._id += 1
        self.proc.stdin.write(json.dumps(
            {"jsonrpc": "2.0", "id": self._id, "method": method, "params": params or {}}
        ) + "\n")
        self.proc.stdin.flush()
        line = self.proc.stdout.readline()
        if not line:
            raise SystemExit(f"server closed the stream; stderr:\n{self.proc.stderr.read()}")
        return json.loads(line)

    def tool(self, name, **arguments):
        reply = self.call("tools/call", {"name": name, "arguments": arguments})
        result = reply["result"]
        text = result["content"][0]["text"]
        if result.get("isError"):
            return None, text
        try:
            return json.loads(text), None
        except json.JSONDecodeError:
            return text, None

    def close(self):
        self.proc.stdin.close()
        self.proc.wait(timeout=10)


def find(node, predicate):
    """Depth-first search over the layout tree."""
    if isinstance(node, dict):
        if predicate(node):
            return node
        for child in node.get("children") or []:
            hit = find(child, predicate)
            if hit:
                return hit
    return None


def main():
    if not BIN.exists():
        raise SystemExit(f"{BIN} not built — run: cargo build -p hiwave-mcp")

    client = Client(BIN)

    handshake = client.call("initialize")["result"]
    assert handshake["serverInfo"]["name"] == "hiwave-mcp", handshake
    assert handshake["protocolVersion"] == "2024-11-05", handshake
    print(f"ok  initialize        {handshake['serverInfo']}")

    names = [t["name"] for t in client.call("tools/list")["result"]["tools"]]
    assert set(names) == {
        "hiwave_open", "hiwave_layout", "hiwave_display_list", "hiwave_style",
        "hiwave_diff", "hiwave_screenshot", "hiwave_status",
    }, names
    print(f"ok  tools/list        {names}")

    # A query before any page is loaded must say so, not crash or return junk.
    for tool in ("hiwave_layout", "hiwave_display_list"):
        value, error = client.tool(tool)
        assert value is None and "hiwave_open" in error, (tool, value, error)
        print(f"ok  {tool}-before-open  refused: {error}")
    value, error = client.tool("hiwave_style", selector=".hero")
    assert value is None and "hiwave_open" in error, (value, error)
    print(f"ok  hiwave_style-before-open  refused: {error}")

    value, error = client.tool("hiwave_open", html=FIXTURE, width=800, height=600)
    assert error is None, error
    assert value["width"] == 800 and value["height"] == 600, value
    print(f"ok  hiwave_open       {value}")

    # The persistence property: the page stays loaded across calls. This is
    # the whole difference from parity-capture, which exits after one render.
    status, error = client.tool("hiwave_status")
    assert error is None and status["loaded"] is True, (status, error)
    print(f"ok  hiwave_status     session survives between calls")

    tree, error = client.tool("hiwave_layout")
    assert error is None, error
    assert tree["viewport"] == {"width": 800, "height": 600}, tree["viewport"]

    # THE ASSERTION THIS CRATE EXISTS FOR.
    # 400 wide + 16 padding either side = 432. 120 + 16 + 16 = 152.
    hero = find(tree["root"], lambda n: (n.get("border_box") or {}).get("width") == 432.0)
    assert hero is not None, "no box of width 432 — expected the .hero border box"
    box = hero["border_box"]
    assert box["height"] == 152.0, box
    print(f"ok  hiwave_layout     .hero border_box = {box['width']}x{box['height']} "
          f"(content-box: 400+2*16 x 120+2*16)")

    # ---- hiwave_display_list ------------------------------------------------
    # The paired assertion, and the reason the tool exists. Layout says the
    # hero's border box is 432x152; paint must be filling THAT rectangle with
    # THAT colour. Checking the two against each other is what lets an agent
    # say "layout is right, paint is wrong" instead of guessing from a pixel
    # diff — a distinction a screenshot structurally cannot make.
    dl, error = client.tool("hiwave_display_list")
    assert error is None, error
    commands = dl["commands"]
    assert dl["count"] == len(commands), dl["count"]

    # #08c is rgb(0, 136, 204) — 0x88 = 136, 0xcc = 204, by hand from the CSS.
    hero_fill = [
        c for c in commands
        if c["op"] == "solid_color"
        and c["color"] == {"r": 0, "g": 136, "b": 204, "a": 1.0}
    ]
    assert len(hero_fill) == 1, f"expected exactly one #08c fill, got {len(hero_fill)}"
    fill_rect = hero_fill[0]["rect"]
    assert fill_rect == {"x": 0.0, "y": 0.0, "width": 432.0, "height": 152.0}, fill_rect
    # background paints over the BORDER box, so paint and layout must agree
    assert fill_rect["width"] == box["width"], (fill_rect, box)
    assert fill_rect["height"] == box["height"], (fill_rect, box)
    print(f"ok  hiwave_display_list  .hero painted rgb(0,136,204) over "
          f"{fill_rect['width']}x{fill_rect['height']} at "
          f"({fill_rect['x']},{fill_rect['y']}) — same rect layout computed")

    # Paint order is load-bearing: the canvas is covered by the hero, which is
    # covered by its text. If this list ever came back unordered, "later
    # commands win" stops being true and every occlusion answer is wrong.
    canvas = [
        c for c in commands
        if c["op"] == "solid_color"
        and c["rect"] == {"x": 0.0, "y": 0.0, "width": 800.0, "height": 600.0}
    ]
    assert len(canvas) == 1, "expected one 800x600 canvas fill (the viewport)"
    text = [c for c in commands if c["op"] == "text" and c["text"] == "Attribution"]
    assert len(text) == 1, f"expected one text command, got {len(text)}"
    assert canvas[0]["index"] < hero_fill[0]["index"] < text[0]["index"], (
        canvas[0]["index"], hero_fill[0]["index"], text[0]["index"])
    print(f"ok  paint order       canvas[{canvas[0]['index']}] < "
          f"hero[{hero_fill[0]['index']}] < text[{text[0]['index']}]")

    # Text properties that come from the cascade and from layout, not from the
    # font: 32px is h1's declared size, x=16 is the hero's padding-left with
    # body margin 0 and h1 margin 0, and 700 is the UA stylesheet's h1 weight.
    # Deliberately NOT asserted: y, ascent, and the advance VALUES — those are
    # platform font metrics, and this runs on a different one than HiWave ships.
    glyphs = text[0]
    assert glyphs["font_size"] == 32.0, glyphs["font_size"]
    assert glyphs["x"] == 16.0, glyphs["x"]
    assert glyphs["font_weight"] == 700, glyphs["font_weight"]
    # The ADVANCE CONTRACT: layout hands paint one advance per character. A
    # null here means paint re-derived its own metrics — the width-drift bug
    # class — and was previously visible only in a trace log.
    assert glyphs["advances"] is not None, "advances absent — paint fell back"
    assert len(glyphs["advances"]) == len("Attribution") == 11, glyphs["advances"]
    print(f"ok  advance contract  {len(glyphs['advances'])} advances for "
          f"{len('Attribution')} chars, font_size={glyphs['font_size']} "
          f"weight={glyphs['font_weight']} x={glyphs['x']}")

    # ---- hiwave_style -------------------------------------------------------
    # Layout and paint both report a CONSEQUENCE. This is the tool that reports
    # the CAUSE, so the assertion is not "a value came back" — it is that the
    # engine names the rule it chose and the rule it rejected.
    style, error = client.tool("hiwave_style", selector=".hero")
    assert error is None, error
    assert style["count"] == 1, f"expected exactly one .hero, got {style['count']}"
    hero_style = style["elements"][0]
    assert hero_style["tag"] == "div" and hero_style["classes"] == ["hero"], hero_style

    # The computed value the engine actually laid out with. 400px is what
    # `.hero` declares; `div { width: 100px }` is later in source order, so a
    # cascade that ignored specificity would report 100px here.
    assert hero_style["computed"]["width"] == "400px", hero_style["computed"]

    width = next(d for d in hero_style["declared"] if d["property"] == "width")
    assert width["computed"] == "400px", width

    # THE ASSERTION hiwave_style EXISTS FOR: the winning rule is named, with
    # its specificity, and it is the CLASS rule — (0,1,0) beats (0,0,1) even
    # though `div` came second in the sheet.
    assert width["winner"]["selector"] == ".hero", width["winner"]
    assert width["winner"]["specificity"] == [0, 1, 0], width["winner"]
    assert width["winner"]["value"] == "400px", width["winner"]
    assert width["winner"]["origin"] == "author", width["winner"]

    # And the rule it BEAT is reported rather than dropped. This is the half
    # that finds "parsed but dead": you cannot see an overridden declaration by
    # looking at the value that survived it.
    assert len(width["overridden"]) == 1, width["overridden"]
    loser = width["overridden"][0]
    assert loser["selector"] == "div", loser
    assert loser["specificity"] == [0, 0, 1], loser
    assert loser["value"] == "100px", loser
    print(f"ok  hiwave_style      width=400px won by {width['winner']['selector']} "
          f"{width['winner']['specificity']} over {loser['selector']} "
          f"{loser['specificity']} (later in source, lower specificity)")

    # The computed value is read off the ComputedStyle the cascade produced,
    # not echoed back from the declaration text: NO rule in the fixture spells
    # `padding-left`. 16px can only come from the engine expanding `padding`.
    assert hero_style["computed"]["padding-left"] == "16px", hero_style["computed"]
    pad_left = next(d for d in hero_style["declared"] if d["property"] == "padding-left")
    # No rule spells `padding-left`, so it cites none — yet it computed to
    # 16px. The value can therefore only have come from the engine expanding
    # the `padding` shorthand, not from echoing declaration text back.
    assert pad_left["winner"] is None, pad_left
    assert pad_left["computed"] == "16px", pad_left
    assert any(d["property"] == "padding" and d["winner"]["value"] == "16px"
               for d in hero_style["declared"]), hero_style["declared"]
    print(f"ok  computed expansion padding-left=16px, winner=None — no rule "
          f"spells it; expanded from `padding: 16px`")

    # "no author rule set this" is an ANSWER, not an omission: it is how an
    # agent tells "the author's rule lost" apart from "the author never wrote
    # one". h1's bold comes from the UA stylesheet, which is a hardcoded match
    # on tag name — real, but with no selector to cite, so the winner is null.
    h1_style, error = client.tool("hiwave_style", selector="h1")
    assert error is None, error
    assert h1_style["count"] == 1, h1_style["count"]
    h1_el = h1_style["elements"][0]
    assert h1_el["computed"]["font-weight"] == "700", h1_el["computed"]
    weight = next(d for d in h1_el["declared"] if d["property"] == "font-weight")
    assert weight["winner"] is None, weight
    assert weight["origin"] == "user-agent-or-initial", weight
    # ...whereas font-size on the same element IS authored, so it cites a rule.
    size = next(d for d in h1_el["declared"] if d["property"] == "font-size")
    assert size["computed"] == "32px", size
    assert size["winner"]["selector"] == "h1", size["winner"]
    assert size["origin"] == "author", size
    print(f"ok  origin split      h1 font-weight=700 winner=None "
          f"(UA, no rule to cite); font-size=32px winner=h1 (author)")

    # ---- the text group -----------------------------------------------------
    # Parity attribution blames text metrics for ~59% of the remaining diff, and
    # until now an agent could not ask which line-height, family or alignment
    # the cascade handed to layout. These are the first three of that group.
    copy_style, error = client.tool("hiwave_style", selector=".copy")
    assert error is None, error
    assert copy_style["count"] == 1, copy_style["count"]
    copy = copy_style["elements"][0]

    # 20 x 1.5 = 30. NO rule in the fixture spells a px line-height, so a
    # serializer echoing declaration text would report "1.5" here. 30px can only
    # come from the engine resolving the multiplier against the computed
    # font-size — the same resolution the line-box code does.
    assert copy["computed"]["font-size"] == "20px", copy["computed"]
    assert copy["computed"]["line-height"] == "30px", copy["computed"]
    lh = next(d for d in copy["declared"] if d["property"] == "line-height")
    assert lh["winner"]["value"] == "1.5", lh    # authored as a bare multiplier
    assert lh["winner"]["selector"] == ".copy", lh
    assert lh["computed"] == "30px", lh          # ...reported as pixels
    print(f"ok  line-height       .copy 20px x 1.5 = 30px — authored '1.5', "
          f"computed {lh['computed']} (resolved, not echoed)")

    # ...and `normal` deliberately stays a KEYWORD. It resolves against the
    # font's own ascent/descent/line-gap, so a px number would look
    # machine-independent and would differ by platform and installed face.
    # .hero declares no line-height and neither does body.
    assert hero_style["computed"]["line-height"] == "normal", hero_style["computed"]
    print(f"ok  normal not faked  .hero line-height=normal (keyword, not px — "
          f"resolving it needs font metrics)")

    # "Nothing declared this" was one answer where CSS has three. Inheritance is
    # the one that points at ANOTHER element, so it is now reported apart from
    # UA-default/initial. The span declares nothing at all: every value below
    # came DOWN the tree from .copy, and an agent chasing a wrong value here
    # needs to be sent to the parent rather than told the property is unset.
    # Queried by id rather than by tag: the fixture now has four spans, and a
    # bare `span` query would silently average four elements into one answer.
    span_style, error = client.tool("hiwave_style", selector="#inherits")
    assert error is None, error
    assert span_style["count"] == 1, span_style["count"]
    span = span_style["elements"][0]
    assert not [d for d in span["declared"] if d["winner"] is not None], span["declared"]
    for prop, expected in (("font-family", "Georgia, serif"),
                           ("text-align", "center")):
        d = next(x for x in span["declared"] if x["property"] == prop)
        assert d["computed"] == expected, d
        assert d["winner"] is None, d
        assert d["origin"] == "inherited", d
    # The distinction is only worth anything if it can still say UA/initial:
    # `display` does not inherit, so the span's block/inline default is NOT
    # inheritance even though .copy also has a value for it.
    disp = next(x for x in span["declared"] if x["property"] == "display")
    assert disp["origin"] == "user-agent-or-initial", disp
    print(f"ok  inherited origin  span font-family='Georgia, serif' "
          f"text-align=center both origin=inherited; display still UA-or-initial")

    # A TRIPWIRE, not an endorsement. The span inherits no line-height in the
    # CASCADE — `compute_style_for_element` seeds font-size/family/weight/
    # style/color/letter-spacing/word-spacing/text-align from the parent but
    # deliberately not line-height, which is inherited one layer later, in
    # build_layout_box ("a value of Normal here reliably means not specified:
    # inherit the parent's computed value", lib.rs ~1541). So LAYOUT lays this
    # span out at 30px while hiwave_style reports `normal` — the tool and the
    # engine disagree on this one property, which is why line-height is NOT
    # counted as answerable for the trench metric even though the .copy
    # assertion above passes. Pinned here so the divergence cannot be
    # forgotten: whoever moves that inheritance into the cascade will see this
    # go red, and should assert "30px" and count the property, not route around
    # it. See trench/digest.md, night 6.
    span_lh = next(x for x in span["declared"] if x["property"] == "line-height")
    assert span_lh["computed"] == "normal", span_lh
    assert span_lh["origin"] == "user-agent-or-initial", span_lh
    print(f"ok  KNOWN DIVERGENCE  span line-height reported '{span_lh['computed']}' "
          f"but laid out at 30px — inherited in build_layout_box, after the trace")

    # ---- font-style: an inherited value, and the value PAINT drew with -------
    # The span declares nothing, so `italic` cannot be an echo of declaration
    # text — it came down the tree from `.copy`. The second half is what makes
    # it count: the paint command carries the italic flag, so the value the
    # tool reports is the value the shaper selected a face with. A tool that
    # agreed with the cascade but not with paint would be worse than a gap.
    fstyle = next(x for x in span["declared"] if x["property"] == "font-style")
    assert fstyle["computed"] == "italic", fstyle
    assert fstyle["winner"] is None, fstyle          # nothing on the span said so
    assert fstyle["origin"] == "inherited", fstyle   # ...`.copy` did
    italic_run = [c for c in commands if c["op"] == "text" and c["text"] == "inherited"]
    assert len(italic_run) == 1, italic_run
    assert italic_run[0]["font_style"] == 1, italic_run[0]   # 0 normal, 1 italic
    # ...and the h1, which inherits nothing italic, is still upright — so the
    # flag tracks the cascade rather than being on for every run.
    assert glyphs["font_style"] == 0, glyphs
    print(f"ok  font-style        span computed=italic origin=inherited (declares "
          f"nothing); paint drew it font_style=1, h1 still 0")

    # ---- letter-spacing: a unit conversion, checked against what layout used --
    # `0.1em` on a 20px element is 2px. No rule spells a px letter-spacing, so
    # an echoing serializer would report `0.1em` — the engine has to resolve
    # the em against the computed font-size to get here.
    spaced_style, error = client.tool("hiwave_style", selector=".spaced")
    assert error is None, error
    assert spaced_style["count"] == 1, spaced_style["count"]
    spaced = spaced_style["elements"][0]
    assert spaced["computed"]["font-size"] == "20px", spaced["computed"]
    assert spaced["computed"]["letter-spacing"] == "2px", spaced["computed"]
    ls = next(d for d in spaced["declared"] if d["property"] == "letter-spacing")
    assert ls["winner"]["value"] == "0.1em", ls      # authored as a multiple of em
    assert ls["winner"]["selector"] == ".spaced", ls
    assert ls["computed"] == "2px", ls               # ...reported as pixels

    # THE HALF THAT MAKES IT COUNT: 2px is the number LAYOUT spaced glyphs by.
    # `.plain` and `.spaced` carry the same text, family and size and differ in
    # exactly one declaration, so every font metric cancels in the difference
    # and what is left is the letter-spacing alone. That is why this is a
    # DELTA and not an absolute advance: the absolute values are this runner's
    # font, the delta is the engine's arithmetic.
    runs = [c for c in commands if c["op"] == "text" and c["text"] == "spacing"]
    assert len(runs) == 2, [c["index"] for c in runs]
    plain_run, spaced_run = sorted(runs, key=lambda c: c["index"])
    assert plain_run["advances"] is not None and spaced_run["advances"] is not None, runs
    assert len(plain_run["advances"]) == len(spaced_run["advances"]) == len("spacing") == 7, runs
    deltas = [round(s - p, 4) for p, s in zip(plain_run["advances"], spaced_run["advances"])]
    assert deltas == [2.0] * 7, deltas
    print(f"ok  letter-spacing    .spaced 0.1em x 20px = 2px (authored '0.1em'); "
          f"every advance is exactly +2.0 over .plain — {deltas}")

    # ---- white-space: implemented, asserted, and NOT counted ------------------
    # A TRIPWIRE, the same shape as line-height above. `white-space` inherits in
    # CSS, but this cascade never seeds it onto an ELEMENT — only onto text
    # nodes, from their immediate parent (build_layout_box). So a span inside a
    # `pre` div computes `normal`, and `inherited_properties` deliberately
    # refuses to label it `inherited`, because the parent did not supply it.
    pre_style, error = client.tool("hiwave_style", selector=".pre")
    assert error is None, error
    pre = pre_style["elements"][0]
    # On the element that declares it, the value is an ECHO of the declaration
    # text — which is exactly why this property is not counted for the metric.
    assert pre["computed"]["white-space"] == "pre", pre["computed"]
    nested_style, error = client.tool("hiwave_style", selector="span.nested")
    assert error is None, error
    assert nested_style["count"] == 1, nested_style["count"]
    nested = nested_style["elements"][0]
    #
    # Verified to flip: seeding `style.white_space = parent.white_space` in
    # compute_style_for_element makes this report `pre` and fails here. Whoever
    # does that must ALSO drop white-space from the exclusion list in
    # `inherited_properties`, or the origin will keep saying UA-or-initial for a
    # value that is by then genuinely inherited — then assert `pre`/`inherited`
    # and count the property. See trench/digest.md, night 7.
    ws = next(x for x in nested["declared"] if x["property"] == "white-space")
    assert ws["computed"] == "normal", ws            # CSS says this should be `pre`
    assert ws["origin"] == "user-agent-or-initial", ws   # NOT "inherited"
    # ...and the consequence is visible in paint, which is what makes this a bug
    # report rather than a reporting quirk: the div's OWN text keeps both of its
    # spaces (4 advances for "a  b"), the nested element's collapses to one.
    texts = {c["text"]: c for c in commands if c["op"] == "text"}
    assert "a  b" in texts, sorted(texts)
    assert len(texts["a  b"]["advances"]) == 4, texts["a  b"]
    assert "c d" in texts and "c  d" not in texts, sorted(texts)
    assert len(texts["c d"]["advances"]) == 3, texts["c d"]
    print(f"ok  KNOWN DIVERGENCE  .pre keeps 'a  b' (4 advances) but its nested span "
          f"collapsed 'c  d' to 'c d' — white-space is not inherited onto elements")

    # A query it cannot honestly answer is refused, not approximated. Matching
    # `div p` needs tree context the trace does not keep, and quietly matching
    # every `p` would answer a different question invisibly.
    value, error = client.tool("hiwave_style", selector="div p")
    assert value is None and "simple selectors only" in error, (value, error)
    print(f"ok  selector guard    refused 'div p'")

    # ---- hiwave_diff --------------------------------------------------------
    # The join. Layout, paint and cascade each report what the engine did;
    # this reports whether that MATCHES what it should have done, per stage,
    # naming every field that disagrees and both values.

    # The agreeing path. `checked` is asserted against the reference file's own
    # length so this cannot pass vacuously: an empty `expect` array would also
    # report agrees=True, and a diff tool that agrees with nothing is the
    # decorative gate this trench exists to avoid.
    hero_ref = json.loads((CASES / "hero" / "spec.layout.json").read_text())
    hero_expect = hero_ref["expect"]
    by_path = {e["path"]: e["value"] for e in hero_expect}
    # The reference must actually pin the number the whole crate is built on.
    assert by_path["root.children[0].children[0].border_box.width"] == 432.0, by_path
    assert by_path["root.children[0].children[0].border_box.height"] == 152.0, by_path

    d, error = client.tool("hiwave_diff", case="hero", stage="layout", reference="spec")
    assert error is None, error
    assert d["checked"] == len(hero_expect) >= 12, (d["checked"], len(hero_expect))
    assert d["differences"] == 0 and d["agrees"] is True, d
    assert d["disagreements"] == [], d["disagreements"]
    print(f"ok  hiwave_diff       hero/layout agrees with the spec reference on "
          f"{d['checked']}/{d['checked']} hand-derived values (incl. border_box 432x152)")

    # Same machinery, the other stage — so `stage` is a real argument and not a
    # single-stage tool wearing a parameter.
    dl_expect = json.loads((CASES / "hero" / "spec.display_list.json").read_text())["expect"]
    d, error = client.tool("hiwave_diff", case="hero", stage="display_list", reference="spec")
    assert error is None, error
    assert d["checked"] == len(dl_expect) >= 17, (d["checked"], len(dl_expect))
    assert d["agrees"] is True, d
    print(f"ok  hiwave_diff       hero/display_list agrees on {d['checked']} values "
          f"(paint order, #08c over 432x152, 32px/700 text at x=16)")

    # THE ASSERTION hiwave_diff EXISTS FOR — the disagreeing path, on a REAL
    # engine bug rather than a contrived one.
    #
    #     .hero { width: 400px }
    #     div   { width: 100px !important }
    #
    # `!important` outranks a normal declaration of the same origin regardless
    # of specificity, so the width is 100px in every browser. RustKit parses
    # the flag, carries it, and then never reads it — the cascade sorts by
    # specificity and source order only — so it computes 400px. The diff must
    # report exactly that, with both numbers, at the two paths width reaches.
    #
    # NOTE FOR WHOEVER FIXES THE CASCADE: when `!important` is honoured this
    # case starts AGREEING and the three asserts below go red. That flip is the
    # signal working, not a broken test — set expected differences to 0.
    d, error = client.tool("hiwave_diff", case="important-width",
                           stage="layout", reference="spec")
    assert error is None, error
    assert d["agrees"] is False, d
    assert d["checked"] == 4 and d["differences"] == 2, d
    widths = {x["path"]: x for x in d["disagreements"]}
    border = widths["root.children[0].children[0].border_box.width"]
    assert border["expected"] == 100.0 and border["actual"] == 400.0, border
    content = widths["root.children[0].children[0].content_rect.width"]
    assert content["expected"] == 100.0 and content["actual"] == 400.0, content
    # ...and the two uncontested values in the same reference still agree, so
    # the disagreement is attributed to one property rather than "everything".
    assert "root.children[0].children[0].border_box.height" not in widths, d
    assert "root.children[0].children[0].border_box.x" not in widths, d
    print(f"ok  hiwave_diff       important-width DISAGREES: border_box.width "
          f"expected {border['expected']} (spec: !important wins), engine computed "
          f"{border['actual']} — 2 of 4, height and x still agree")

    # The diff runs its case in its own engine, so it must not have disturbed
    # the page the session has open. Re-asserting the original number is the
    # cheapest way to prove it.
    tree_again, error = client.tool("hiwave_layout")
    assert error is None, error
    hero_again = find(tree_again["root"],
                      lambda n: (n.get("border_box") or {}).get("width") == 432.0)
    assert hero_again is not None, "the diff clobbered the open session's page"
    print(f"ok  session isolation open page still 432x152 after three diffs")

    # Inputs it cannot honestly answer are refused, not guessed at. `case` and
    # `reference` are joined onto a directory, so a name that escapes it is a
    # bug worth a test rather than a comment.
    for args, expect in (
        (dict(case="hero", stage="style", reference="spec"), "unknown stage"),
        (dict(case="no-such-case", reference="spec"), "no such case"),
        (dict(case="hero", reference="no-such-reference"), "no reference"),
        (dict(case="../../etc", reference="spec"), "must be a plain name"),
        (dict(case="hero"), "`reference` is required"),
    ):
        value, error = client.tool("hiwave_diff", **args)
        assert value is None and expect in error, (args, value, error)
    print(f"ok  diff guards       unknown stage, unknown case, unknown reference, "
          f"path escape and missing argument all refused")

    shot, error = client.tool("hiwave_screenshot")
    assert error is None, error
    frame = Path(shot["path"])
    assert frame.exists() and frame.stat().st_size > 0, shot
    print(f"ok  hiwave_screenshot {frame.stat().st_size} bytes at {shot['format']}")

    # Ambiguous input is refused rather than silently resolved — a typo in one
    # argument should not look like a render bug.
    value, error = client.tool("hiwave_open", html="<p>x", path="/tmp/nope.html")
    assert value is None and "not both" in error, (value, error)
    print(f"ok  argument guard    {error}")

    client.close()
    print("\nPASS: hiwave-mcp serves the engine's computed layout, its paint "
          "commands, the cascade behind them, AND whether any of it agrees "
          "with a committed reference")


if __name__ == "__main__":
    sys.exit(main())
