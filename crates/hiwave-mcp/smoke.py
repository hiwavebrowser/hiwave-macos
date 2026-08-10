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
pre { margin: 0 }
.framed    { width: 200px; height: 40px; font-size: 20px; border: 0.25em solid #c60 }
.hairline  { width: 200px; height: 40px; border: 2px solid }
.overruled { width: 200px; height: 40px; border: 3px solid #093; border-top-width: 9px }
.bordered  { width: 200px; height: 40px; padding: 10px; border: 5px solid #333;
             box-sizing: border-box }
.content   { width: 200px; height: 40px; padding: 10px; border: 5px solid #333 }
.host      { width: 300px }
.floater   { position: absolute; width: 50px; height: 40px }
.clipped   { width: 260px; overflow: hidden }
.unclipped { width: 240px }
.kid       { margin-top: 10px; height: 5px; margin-bottom: 20px }
.faded     { width: 100px; height: 10px; background: #f00; opacity: 0.5 }
div { box-sizing: content-box; position: static; white-space: normal }
</style></head><body><div class="hero"><h1>Attribution</h1></div>\
<div class="copy"><span id="inherits">inherited</span></div>\
<div class="plain"><span>spacing</span></div>\
<div class="spaced"><span>spacing</span></div>\
<div class="pre">a  b<span class="nested">c  d</span></div>\
<pre>x  y</pre>\
<div class="framed"></div><div class="hairline"></div><div class="overruled"></div>\
<div class="bordered"></div><div class="content"></div>\
<div class="host"><div class="floater"></div></div>\
<div class="clipped"><div class="kid"></div></div>\
<div class="unclipped"><div class="kid"></div></div>\
<div class="faded"></div>\
</body></html>"""


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
    assert pad_left["computed"] == "16px", pad_left
    # Nights 2-7 reported `"winner": null` here and named it the one place this
    # output could mislead: literally true (no rule spells `padding-left`) and
    # read by anyone as "nothing set this", when `padding: 16px` plainly did.
    # It now cites the declaration that actually wrote the field, and says it
    # did so under another name. The value is still the engine's expansion, not
    # an echo — the winner's own value is "16px" for the SHORTHAND.
    assert pad_left["winner"]["property"] == "padding", pad_left["winner"]
    assert pad_left["winner"]["via_shorthand"] is True, pad_left["winner"]
    assert pad_left["winner"]["selector"] == ".hero", pad_left["winner"]
    assert pad_left["origin"] == "author", pad_left
    print(f"ok  computed expansion padding-left=16px, cited to `padding: "
          f"{pad_left['winner']['value']}` on {pad_left['winner']['selector']} "
          f"(via_shorthand) — no rule spells the longhand")

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

    # This was night 6's KNOWN DIVERGENCE and is now the assertion that counts
    # `line-height`. The span inherits no line-height in the CASCADE —
    # `compute_style_for_element` seeds font-size/family/weight/style/color/
    # letter-spacing/word-spacing/text-align from the parent but deliberately
    # not line-height, which is inherited one layer later, in build_layout_box.
    # The trace used to be snapshotted before that, so the tool said `normal`
    # while layout used 30px. The trace is now re-read AFTER those adjustments
    # (`amend_trace_for_layout_style`), so the reported value is the one layout
    # was handed. Note what did NOT change: the inheritance itself, which is
    # untouched engine behaviour. See trench/digest.md, night 10.
    #
    # 30px is hand-derivable and carries no font metrics: `.copy` declares
    # `font-size: 20px; line-height: 1.5`, and 20 x 1.5 = 30. The span declares
    # nothing at all (asserted above), so 30px can only have come down the tree
    # — it is neither an echo of any declaration on this element nor a value a
    # UA default could supply.
    span_lh = next(x for x in span["declared"] if x["property"] == "line-height")
    assert span_lh["computed"] == "30px", span_lh
    assert span_lh["winner"] is None, span_lh          # no rule on this element
    assert span_lh["origin"] == "inherited", span_lh   # ...and it says where it came from

    # The half that makes it COUNT rather than merely report (BASELINE.md
    # clause 3): the reported value is cross-checked against the geometry
    # layout produced. The `.copy` div's only text sits INSIDE this span, so
    # the line box it generates is driven by the span's line-height and by
    # nothing else. If the span had kept `normal` — the value the tool used to
    # report — this box could not be 30 tall.
    inherited_text = find(tree["root"], lambda n: n.get("text") == "inherited")
    assert inherited_text is not None, tree["root"]
    assert inherited_text["rect"]["height"] == 30.0, inherited_text

    # ...and the control, which proves 30.0 is not simply what any 20px text
    # box measures here. `.plain` declares the same `font-size: 20px` and the
    # same family but no line-height, so its line box is `normal` — a
    # FONT-DERIVED number, which is why it is asserted to differ from 30 rather
    # than pinned to a value. Pinning it would make this gate report a
    # text-stack difference as an engine regression, which the header of this
    # file rules out.
    plain_text = find(tree["root"], lambda n: n.get("text") == "spacing")
    assert plain_text is not None, tree["root"]
    assert plain_text["rect"]["height"] != 30.0, plain_text
    print(f"ok  line-height inherited  span declares nothing, reports 30px origin=inherited "
          f"(.copy 20px x 1.5) and its line box IS 30.0 — `normal` control measures "
          f"{plain_text['rect']['height']}")

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

    # ---- white-space: COUNTED, on the standard night 9 set for keywords ------
    # `white-space` is a keyword, so no unit conversion or shorthand expansion
    # can show the value was computed rather than echoed — exactly the position
    # `box-sizing` and `position` were in on night 9, and `overflow-x` on night
    # 11. That night fixed the standard for keyword properties, and it is the
    # standard applied here:
    #
    #   (a) a LATER, LOWER-specificity rule matching the same element declares
    #       the other value and loses. So "echo the last matching declaration"
    #       and "report what the cascade decided" give DIFFERENT answers, and
    #       only the cascade's is right. `div { white-space: normal }` is last
    #       in the fixture's sheet and is load-bearing for precisely this.
    #   (b) the reported value is cross-checked against a consequence the engine
    #       produced, which differs observably between the two settings.
    #
    # Night 12 marked this property uncountable on a different and stricter
    # test — that the value must arrive by shorthand, inheritance or a UA
    # default — and all three of those are genuinely absent (both gaps are
    # still pinned as tripwires below). But that test is not the one under
    # which two keyword properties were already counted: `.floater`'s
    # `position: absolute` is just as much an echo of its own declaration text.
    # Applied consistently, (a)+(b) hold here, and they hold with a STRONGER
    # (b) than either prior keyword had — the consequence is read out of the
    # display list, an export path independent of the cascade the value came
    # from, rather than out of geometry.
    pre_style, error = client.tool("hiwave_style", selector=".pre")
    assert error is None, error
    pre = pre_style["elements"][0]
    assert pre["computed"]["white-space"] == "pre", pre["computed"]
    # (a) the cascade decided it, and the rule it beat is named rather than
    # dropped. Echoing the final matching declaration would say `normal` here.
    pre_ws = next(x for x in pre["declared"] if x["property"] == "white-space")
    assert pre_ws["winner"]["selector"] == ".pre", pre_ws["winner"]
    assert pre_ws["winner"]["specificity"] == [0, 1, 0], pre_ws["winner"]
    assert pre_ws["winner"]["value"] == "pre", pre_ws["winner"]
    assert pre_ws["origin"] == "author", pre_ws
    assert len(pre_ws["overridden"]) == 1, pre_ws["overridden"]
    pre_loser = pre_ws["overridden"][0]
    assert pre_loser["selector"] == "div", pre_loser
    assert pre_loser["specificity"] == [0, 0, 1], pre_loser
    assert pre_loser["value"] == "normal", pre_loser
    # (b) and layout/paint really used `pre`: the div's own text keeps BOTH of
    # its spaces. The control is on the same page in the same run — two
    # elements whose text collapses to one space — so this is not an engine
    # that always keeps whitespace.
    texts = {c["text"]: c for c in commands if c["op"] == "text"}
    assert "a  b" in texts and "a b" not in texts, sorted(texts)
    assert len(texts["a  b"]["advances"]) == 4, texts["a  b"]
    assert len(texts["c d"]["advances"]) == 3, texts["c d"]   # collapsed control
    print(f"ok  white-space      .pre computed=pre won by .pre [0, 1, 0] over a LATER "
          f"div [0, 0, 1] normal; paint kept both spaces — 'a  b' has "
          f"{len(texts['a  b']['advances'])} advances against 3 for the collapsed control")
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
    assert "a  b" in texts, sorted(texts)
    assert len(texts["a  b"]["advances"]) == 4, texts["a  b"]
    assert "c d" in texts and "c  d" not in texts, sorted(texts)
    assert len(texts["c d"]["advances"]) == 3, texts["c d"]
    print(f"ok  KNOWN DIVERGENCE  .pre keeps 'a  b' (4 advances) but its nested span "
          f"collapsed 'c  d' to 'c d' — white-space is not inherited onto elements")

    # A SECOND, INDEPENDENT tripwire on the same property, and the one that bites
    # real pages. The divergence above needs an author `white-space: pre` to be
    # visible at all; this one needs no CSS whatsoever. Every browser's UA sheet
    # gives `pre` white-space: pre. RustKit's UA arm for "pre" sets display,
    # font-family and margins and then says, in the source,
    # `// white-space: pre (not implemented)` — so a bare <pre> collapses its
    # whitespace like a div. Night 11 named this property's gap as missing
    # ELEMENT inheritance; that is real but it is not the whole gap, and it is
    # not the half a page hits without stylesheets.
    #
    # This assertion is NOT the one that counts the property — the counted one is
    # above, on `.pre`, where the cascade had a real decision to make. Here the
    # engine defaulted rather than computed: `normal` on a bare <pre> is the
    # INITIAL value, and asserting an initial value proves nothing about the
    # cascade. Keep the two straight. All three routes to a white-space value
    # that does not depend on an author rule are still absent — a shorthand
    # (nothing sets white-space; `shorthands_setting` maps only the `font`
    # family), inheritance (absent for elements, above), and a UA default
    # (absent, here) — which is why an UNSTYLED page still gets the wrong
    # answer from the engine, and why both tripwires stay.
    bare_pre, error = client.tool("hiwave_style", selector="pre")
    assert error is None, error
    assert bare_pre["count"] == 1, bare_pre["count"]
    bare = bare_pre["elements"][0]
    # CSS says `pre`, and no author rule in the fixture touches white-space here.
    assert bare["computed"]["white-space"] == "normal", bare["computed"]
    bare_ws = next(x for x in bare["declared"] if x["property"] == "white-space")
    assert bare_ws["winner"] is None, bare_ws          # nothing declared it...
    assert bare_ws["origin"] == "user-agent-or-initial", bare_ws
    # ...and the UA sheet did reach this element for OTHER properties, so the
    # `normal` above is a missing UA declaration and not a UA sheet that missed
    # the element entirely. monospace + display:block are that same arm's work.
    assert bare["computed"]["display"] == "block", bare["computed"]
    # The consequence, in paint: the double space is gone. Whoever implements the
    # UA default will see this line go red — assert 'x  y' with 4 advances then,
    # and re-check clause 2 before counting the property (a UA default is still
    # not a computed value).
    assert "x y" in texts and "x  y" not in texts, sorted(texts)
    assert len(texts["x y"]["advances"]) == 3, texts["x y"]
    print(f"ok  KNOWN GAP        a BARE <pre> computes white-space=normal (winner=None) "
          f"and collapses 'x  y' to 'x y' — the UA sheet reached it (display=block, "
          f"monospace) but has no white-space declaration to give")

    # ---- the shorthand group: border-top-width and border-top-color ----------
    # These are in the diagnosis set precisely because almost nobody writes the
    # longhands: they are set by `border`, which is what forces provenance to
    # survive a shorthand. Both halves have to hold — the value the engine
    # computed, AND a citation that names the rule that really wrote it.
    framed_style, error = client.tool("hiwave_style", selector=".framed")
    assert error is None, error
    assert framed_style["count"] == 1, framed_style["count"]
    framed = framed_style["elements"][0]

    # 0.25em on a 20px element is 5px. Nothing in the fixture spells a px
    # border width, and nothing spells `border-top-width` at all: 5px requires
    # the engine to expand the shorthand AND resolve the em against the
    # computed font-size. Two conversions away from the declaration text.
    assert framed["computed"]["font-size"] == "20px", framed["computed"]
    assert framed["computed"]["border-top-width"] == "5px", framed["computed"]
    btw = next(d for d in framed["declared"] if d["property"] == "border-top-width")
    assert btw["computed"] == "5px", btw
    assert btw["winner"]["property"] == "border", btw["winner"]
    assert btw["winner"]["via_shorthand"] is True, btw["winner"]
    assert btw["winner"]["value"] == "0.25em solid #c60", btw["winner"]
    assert btw["winner"]["selector"] == ".framed", btw["winner"]
    assert btw["origin"] == "author", btw

    # #c60 is rgb(204, 102, 0) — 0xcc = 204, 0x66 = 102, by hand from the CSS.
    assert framed["computed"]["border-top-color"] == "rgba(204, 102, 0, 1)", framed["computed"]
    btc = next(d for d in framed["declared"] if d["property"] == "border-top-color")
    assert btc["winner"]["property"] == "border", btc["winner"]
    assert btc["winner"]["via_shorthand"] is True, btc["winner"]
    assert btc["origin"] == "author", btc

    # THE HALF THAT MAKES THEM COUNT: 5px is the width LAYOUT reserved and the
    # height PAINT drew. 200 content + 5 border either side = 210, and the top
    # border is a full-width band 5px tall in exactly that colour.
    framed_box = find(tree["root"],
                      lambda n: (n.get("border_box") or {}).get("width") == 210.0)
    assert framed_box is not None, "no 210-wide box — expected .framed (200 + 2*5)"
    assert framed_box["border"]["top"] == 5.0, framed_box["border"]
    assert framed_box["content_rect"]["width"] == 200.0, framed_box["content_rect"]
    bands = [c for c in commands
             if c["op"] == "solid_color"
             and c["color"] == {"r": 204, "g": 102, "b": 0, "a": 1.0}
             and c["rect"]["height"] == 5.0]
    # Two: the top band and the bottom one. The left and right sides are 5 WIDE
    # and 50 tall, so they cannot be mistaken for these.
    assert len(bands) == 2, [c["rect"] for c in bands]
    top_band = [c for c in bands if c["rect"]["y"] == framed_box["border_box"]["y"]]
    assert len(top_band) == 1, ([c["rect"] for c in bands], framed_box["border_box"])
    assert top_band[0]["rect"]["width"] == 210.0, top_band[0]["rect"]
    print(f"ok  border shorthand  .framed border-top-width=5px (0.25em x 20px) and "
          f"border-top-color=rgba(204, 102, 0, 1), both cited to `border` on .framed; "
          f"layout reserved 5.0 and paint drew a 210.0x5.0 band")

    # THE ASSERTION THE MEASUREMENT EXISTS FOR. `border: 2px solid` carries no
    # colour, so `parse_border_shorthand` returns None for it and the cascade
    # leaves border_top_color alone. A hand-written table saying "`border` sets
    # the four widths and the four colours" would cite this rule as the source
    # of a colour it never wrote — a plausible, confident lie. Attribution is
    # measured by running the applying function instead, so the width is cited
    # and the colour is not.
    hair_style, error = client.tool("hiwave_style", selector=".hairline")
    assert error is None, error
    hair = hair_style["elements"][0]
    hw = next(d for d in hair["declared"] if d["property"] == "border-top-width")
    assert hw["computed"] == "2px", hw
    assert hw["winner"]["property"] == "border", hw["winner"]      # the width: cited
    hc = next(d for d in hair["declared"] if d["property"] == "border-top-color")
    assert hc["winner"] is None, hc                                # the colour: NOT
    assert hc["origin"] == "user-agent-or-initial", hc
    print(f"ok  no false citation `border: 2px solid` cites the width and NOT "
          f"border-top-color (the declaration carried no colour)")

    # ...and the ordering a merged property list has to get right: a longhand
    # AFTER a shorthand beats it, and the shorthand is reported as beaten
    # rather than dropped. 9px, not the shorthand's 3px, is also what layout
    # reserved — 200 + 3 + 3 wide, 40 + 9 + 3 tall.
    over_style, error = client.tool("hiwave_style", selector=".overruled")
    assert error is None, error
    over = over_style["elements"][0]
    ow = next(d for d in over["declared"] if d["property"] == "border-top-width")
    assert ow["computed"] == "9px", ow
    assert ow["winner"]["property"] == "border-top-width", ow["winner"]
    assert ow["winner"]["via_shorthand"] is False, ow["winner"]
    assert len(ow["overridden"]) == 1, ow["overridden"]
    assert ow["overridden"][0]["property"] == "border", ow["overridden"]
    assert ow["overridden"][0]["via_shorthand"] is True, ow["overridden"]
    over_box = find(tree["root"],
                    lambda n: (n.get("border_box") or {}).get("width") == 206.0)
    assert over_box is not None, "no 206-wide box — expected .overruled (200 + 2*3)"
    assert over_box["border"] == {"top": 9.0, "right": 3.0, "bottom": 3.0, "left": 3.0}, \
        over_box["border"]
    assert over_box["border_box"]["height"] == 52.0, over_box["border_box"]  # 40 + 9 + 3
    print(f"ok  longhand wins     .overruled border-top-width=9px beats `border: 3px "
          f"solid #093`, which is reported in overridden; layout reserved "
          f"{over_box['border']} and a {over_box['border_box']['height']}-tall box")

    # ---- the box group: box-sizing and position ------------------------------
    # These decide the SHAPE of the box rather than a number inside it. Both are
    # keywords, so no unit conversion can prove the value was computed rather
    # than echoed; two other things do. First, `div { box-sizing: content-box;
    # position: static }` sits LAST in the sheet and matches the same elements,
    # so the last declaration a naive serializer would echo is the losing one.
    # Second — and this is what makes them count — the reported value is
    # cross-checked against the geometry LAYOUT produced, which differs
    # observably between the two settings.
    bordered_style, error = client.tool("hiwave_style", selector=".bordered")
    assert error is None, error
    assert bordered_style["count"] == 1, bordered_style["count"]
    bordered = bordered_style["elements"][0]

    assert bordered["computed"]["box-sizing"] == "border-box", bordered["computed"]
    bs = next(d for d in bordered["declared"] if d["property"] == "box-sizing")
    assert bs["winner"]["selector"] == ".bordered", bs["winner"]
    assert bs["winner"]["specificity"] == [0, 1, 0], bs["winner"]
    assert bs["winner"]["value"] == "border-box", bs["winner"]
    assert bs["origin"] == "author", bs
    # The rule it beat is the LAST one in the sheet, so "echo the final
    # declaration" and "report what the cascade decided" give different answers
    # here, and only the second one is right.
    assert len(bs["overridden"]) == 1, bs["overridden"]
    assert bs["overridden"][0]["selector"] == "div", bs["overridden"]
    assert bs["overridden"][0]["value"] == "content-box", bs["overridden"]

    # THE HALF THAT MAKES IT COUNT: `border-box` means the declared 200px IS
    # the border box, so the content shrinks to 200 - 2*10 padding - 2*5 border
    # = 170, and the height to 40 - 20 - 10 = 10.
    bordered_box = find(tree["root"],
                        lambda n: (n.get("border_box") or {}).get("width") == 200.0)
    assert bordered_box is not None, "no 200-wide box — expected .bordered (border-box)"
    assert bordered_box["border_box"]["height"] == 40.0, bordered_box["border_box"]
    assert bordered_box["content_rect"]["width"] == 170.0, bordered_box["content_rect"]
    assert bordered_box["content_rect"]["height"] == 10.0, bordered_box["content_rect"]

    # ...and the control proves the engine is not simply always doing that.
    # `.content` declares the SAME 200px width, the same padding and the same
    # border, and differs only in box-sizing: 200 + 2*10 + 2*5 = 230.
    content_style, error = client.tool("hiwave_style", selector=".content")
    assert error is None, error
    assert content_style["elements"][0]["computed"]["box-sizing"] == "content-box", \
        content_style["elements"][0]["computed"]
    content_box = find(tree["root"],
                       lambda n: (n.get("border_box") or {}).get("width") == 230.0)
    assert content_box is not None, "no 230-wide box — expected .content (content-box)"
    assert content_box["content_rect"]["width"] == 200.0, content_box["content_rect"]
    assert content_box["border_box"]["height"] == 70.0, content_box["border_box"]  # 40+20+10
    print(f"ok  box-sizing        .bordered border-box: declared 200px IS the border box, "
          f"content 170.0x10.0; .content content-box, same declarations, border box 230.0x70.0")

    # `position` — the value that decides whether the box is in flow at all.
    floater_style, error = client.tool("hiwave_style", selector=".floater")
    assert error is None, error
    assert floater_style["count"] == 1, floater_style["count"]
    floater = floater_style["elements"][0]
    assert floater["computed"]["position"] == "absolute", floater["computed"]
    pos = next(d for d in floater["declared"] if d["property"] == "position")
    assert pos["winner"]["selector"] == ".floater", pos["winner"]
    assert pos["winner"]["specificity"] == [0, 1, 0], pos["winner"]
    assert pos["origin"] == "author", pos
    assert len(pos["overridden"]) == 1, pos["overridden"]
    assert pos["overridden"][0]["selector"] == "div", pos["overridden"]
    assert pos["overridden"][0]["value"] == "static", pos["overridden"]

    # THE HALF THAT MAKES IT COUNT: out of flow. `.host` is auto-height and its
    # only child is 40px tall, so a static child would make it 40; absolute
    # takes the child out of flow and the parent collapses to 0. The child is
    # still really 40 tall, and the next block starts at the host's OWN y —
    # i.e. the 40px box reserved no space for itself.
    host_box = find(tree["root"],
                    lambda n: (n.get("border_box") or {}).get("width") == 300.0)
    assert host_box is not None, "no 300-wide box — expected .host"
    assert host_box["border_box"]["height"] == 0.0, host_box["border_box"]
    assert len(host_box["children"]) == 1, host_box["children"]
    assert host_box["children"][0]["border_box"]["height"] == 40.0, host_box["children"][0]
    clipped_box = find(tree["root"],
                       lambda n: (n.get("border_box") or {}).get("width") == 260.0)
    assert clipped_box is not None, "no 260-wide box — expected .clipped"
    assert clipped_box["border_box"]["y"] == host_box["border_box"]["y"], \
        (clipped_box["border_box"], host_box["border_box"])
    print(f"ok  position          .floater absolute (beat `div{{position:static}}`, last in "
          f"sheet); .host is 0.0 tall around a 40.0-tall child and the next block starts at "
          f"the same y — out of flow")

    # ---- overflow-x: COUNTED. opacity: reported, NOT counted ----------------
    # `overflow-x` is cited correctly — no rule spells the longhand, and the
    # citation names the `overflow` shorthand that really wrote it. That is
    # clauses 1 and 2 (a shorthand expansion, so the value is not an echo of any
    # declaration text).
    clipped_style, error = client.tool("hiwave_style", selector=".clipped")
    assert error is None, error
    clipped = clipped_style["elements"][0]
    assert clipped["computed"]["overflow-x"] == "hidden", clipped["computed"]
    ox = next(d for d in clipped["declared"] if d["property"] == "overflow-x")
    assert ox["winner"]["property"] == "overflow", ox["winner"]
    assert ox["winner"]["via_shorthand"] is True, ox["winner"]
    assert ox["winner"]["selector"] == ".clipped", ox["winner"]

    # THE HALF THAT MAKES IT COUNT — clause 3: the value the tool reports is the
    # value LAYOUT used. Night 9 looked for that here and concluded it was not
    # observable, but it measured the kid's TOP margin, and
    # `should_collapse_with_first_child` is never called by the block path.
    # `should_collapse_with_last_child` IS (rustkit-layout/src/lib.rs), and it
    # asks `establishes_bfc`, which reads `overflow_x` directly
    # (margin_collapse.rs). So the LAST child's bottom margin is where the
    # engine spends the value.
    #
    # `.clipped` and `.unclipped` hold the SAME `.kid` and differ in exactly one
    # declaration — `overflow: hidden`. The kid is margin-top 10, height 5,
    # margin-bottom 20:
    #
    #   .clipped   overflow:hidden -> a BFC -> no collapse-through, so the
    #              pending 20px bottom margin is materialised INSIDE the parent:
    #              10 + 5 + 20 = 35
    #   .unclipped overflow:visible -> not a BFC -> collapse-through allowed and
    #              the pending margin is not added:            10 + 5 = 15
    #
    # 20.0 of geometry, attributable to one keyword. NOTE what is NOT claimed:
    # neither number is Chrome's. Chrome collapses the top margin out too (the
    # engine does not — `should_collapse_with_first_child` is unwired) and
    # adjoins the escaping bottom margin to the parent rather than dropping it
    # (ledgered in the block-height comment). Both are pre-existing residuals,
    # unrelated to this property. What is asserted is the engine's own
    # behaviour and, in particular, the DIFFERENCE that overflow-x alone causes.
    unclipped_box = find(tree["root"],
                         lambda n: (n.get("border_box") or {}).get("width") == 240.0)
    assert unclipped_box is not None, "no 240-wide box — expected .unclipped"
    clipped_h = clipped_box["border_box"]["height"]
    unclipped_h = unclipped_box["border_box"]["height"]
    assert clipped_h == 35.0, clipped_box["border_box"]
    assert unclipped_h == 15.0, unclipped_box["border_box"]
    # ...and the gap is exactly the kid's bottom margin, not some other drift:
    # the two children are identical, so anything else differing would show up
    # here instead.
    assert clipped_h - unclipped_h == 20.0, (clipped_h, unclipped_h)
    kid_offset = (clipped_box["children"][0]["border_box"]["y"]
                  - clipped_box["border_box"]["y"])
    unclipped_kid_offset = (unclipped_box["children"][0]["border_box"]["y"]
                            - unclipped_box["border_box"]["y"])
    assert kid_offset == unclipped_kid_offset == 10.0, (kid_offset, unclipped_kid_offset)
    assert clipped_box["children"][0]["border_box"]["height"] == 5.0, clipped_box["children"][0]
    print(f"ok  overflow-x       .clipped overflow-x=hidden cited to `overflow`; it establishes "
          f"a BFC so the kid's 20px bottom margin lands INSIDE — {clipped_h} tall against "
          f"{unclipped_h} for .unclipped, same child, one keyword apart")

    # THE LIMIT, still pinned: layout spends overflow-x, PAINT DOES NOT. There
    # is no push_clip for overflow anywhere in the display list, so an agent
    # reading `hidden` learns what layout did with it and must NOT conclude the
    # content was clipped. Counted for the value (as `position` was on night 9,
    # for the value and not its consequences), with the paint half named rather
    # than implied. Whoever implements clipping will see this go red and should
    # then assert the clip instead of deleting the line.
    assert not [c for c in commands if c["op"] in ("push_clip", "pop_clip")], \
        [c for c in commands if c["op"] in ("push_clip", "pop_clip")]
    print(f"ok  KNOWN GAP        ...but paint pushes no clip for it — overflow-x is answered "
          f"for what LAYOUT did, not for whether anything was clipped")

    # `opacity` is the clearer case: the cascade CLAMPS it, so 0.5 is a computed
    # number rather than an echo, and the clamp is the engine's own. But paint
    # never reads it for anything but images, and the renderer discards it even
    # there (`opacity: _` in the Image arm). So the fill for .faded comes out at
    # full alpha, and a tool reporting 0.5 would be describing a transparency
    # the engine does not draw.
    faded_style, error = client.tool("hiwave_style", selector=".faded")
    assert error is None, error
    faded = faded_style["elements"][0]
    assert faded["computed"]["opacity"] == "0.5", faded["computed"]
    faded_fill = [c for c in commands
                  if c["op"] == "solid_color"
                  and c["color"]["r"] == 255 and c["color"]["g"] == 0 and c["color"]["b"] == 0]
    assert len(faded_fill) == 1, [c["rect"] for c in faded_fill]
    assert faded_fill[0]["rect"]["width"] == 100.0, faded_fill[0]["rect"]
    # The tripwire: full alpha, for a half-transparent element.
    assert faded_fill[0]["color"]["a"] == 1.0, faded_fill[0]["color"]
    print(f"ok  KNOWN GAP        .faded opacity=0.5 computed, but paint filled it at "
          f"a={faded_fill[0]['color']['a']} — opacity never reaches the solid-colour path, "
          f"so it is reported, not counted")

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
