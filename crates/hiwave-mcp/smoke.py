#!/usr/bin/env python3
"""Smoke test for hiwave-mcp: drive the real protocol over stdio.

Run:  cargo build -p hiwave-mcp && python3 crates/hiwave-mcp/smoke.py

This asserts the property the crate exists for — that an agent can read what
the engine COMPUTED, not just look at what it painted. The fixture is chosen
so the correct answer is derivable from the CSS by hand:

    .hero { width: 400px; height: 120px; padding: 16px }

with the default content-box sizing, the border box must be 432 x 152. A
pixel diff can tell you the hero looks wrong; only the layout tree can tell
you whether layout or paint got it wrong.
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
        "hiwave_open", "hiwave_layout", "hiwave_screenshot", "hiwave_status",
    }, names
    print(f"ok  tools/list        {names}")

    # A query before any page is loaded must say so, not crash or return junk.
    value, error = client.tool("hiwave_layout")
    assert value is None and "hiwave_open" in error, (value, error)
    print(f"ok  layout-before-open  refused: {error}")

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
    print("\nPASS: hiwave-mcp serves the engine's computed layout over MCP")


if __name__ == "__main__":
    sys.exit(main())
