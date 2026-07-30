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
"""
import json
import subprocess
import sys
from pathlib import Path

BIN = Path(__file__).resolve().parents[2] / "target" / "debug" / "hiwave-mcp"

FIXTURE = """<!DOCTYPE html><html><head><style>
body { margin: 0 }
.hero { width: 400px; height: 120px; padding: 16px; background: #08c }
h1 { font-size: 32px; margin: 0 }
</style></head><body><div class="hero"><h1>Attribution</h1></div></body></html>"""


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
        "hiwave_open", "hiwave_layout", "hiwave_display_list",
        "hiwave_screenshot", "hiwave_status",
    }, names
    print(f"ok  tools/list        {names}")

    # A query before any page is loaded must say so, not crash or return junk.
    for tool in ("hiwave_layout", "hiwave_display_list"):
        value, error = client.tool(tool)
        assert value is None and "hiwave_open" in error, (tool, value, error)
        print(f"ok  {tool}-before-open  refused: {error}")

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
    print("\nPASS: hiwave-mcp serves the engine's computed layout AND its paint "
          "commands over MCP")


if __name__ == "__main__":
    sys.exit(main())
