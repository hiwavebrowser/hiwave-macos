#!/usr/bin/env python3
"""Repro: does unitless line-height:1.5 inherit from html to descendants?
Chrome computes h1 (font-size 2em=32px) line-height = 48px (1.5 factor).
"""
import subprocess, json, os, sys, tempfile, pathlib

REPO = pathlib.Path(__file__).resolve().parents[2]  # hiwave-macos
CAP = REPO / "target" / "release" / "parity-capture"

CASES = {
    "inline-html-lh": """<!DOCTYPE html><html style="line-height:1.5;font-size:16px">
<head><meta charset=utf-8></head><body style="margin:0">
<h1 style="margin:0;font-size:2em">Heading</h1>
<p style="margin:0">Body paragraph text here</p>
</body></html>""",
    "inline-body-lh": """<!DOCTYPE html><html style="font-size:16px"><head><meta charset=utf-8></head>
<body style="margin:0;line-height:1.5">
<h1 style="margin:0;font-size:2em">Heading</h1>
<p style="margin:0">Body paragraph text here</p>
</body></html>""",
    "no-lh": """<!DOCTYPE html><html style="font-size:16px"><head><meta charset=utf-8></head>
<body style="margin:0">
<h1 style="margin:0;font-size:2em">Heading</h1>
<p style="margin:0">Body paragraph text here</p>
</body></html>""",
    "style-block-lh": """<!DOCTYPE html><html><head><meta charset=utf-8>
<style>html{line-height:1.5;font-size:16px} *{margin:0} h1{font-size:2em}</style></head>
<body><h1>Heading</h1><p>Body paragraph text here</p></body></html>""",
}

def find(node, tag, out):
    if isinstance(node, dict):
        t = node.get("tag") or node.get("type")
        # element nodes carry border_box + children; detect by presence of h1 text
        bb = node.get("border_box")
        if bb and node.get("children"):
            for c in node["children"]:
                if isinstance(c, dict) and c.get("type") == "text":
                    out.append((c.get("text","")[:12], bb.get("height")))
        for c in node.get("children", []) or []:
            find(c, tag, out)

def run(name, html):
    with tempfile.NamedTemporaryFile("w", suffix=".html", delete=False, dir=str(REPO)) as f:
        f.write(html); path = f.name
    outdir = REPO / "parity-tests" / "repro" / "lh-out"
    outdir.mkdir(parents=True, exist_ok=True)
    layout = outdir / f"{name}.layout.json"
    cmd = [str(CAP), "--html-file", path, "--width", "800", "--height", "600",
           "--dump-frame", str(outdir / f"{name}.ppm"), "--dump-layout", str(layout)]
    r = subprocess.run(cmd, capture_output=True, text=True, timeout=60, cwd=str(REPO))
    os.unlink(path)
    if r.returncode != 0:
        print(f"{name}: FAIL {r.stderr[:200]}"); return
    d = json.load(open(layout))
    out = []
    find(d.get("root", d), "h1", out)
    print(f"{name}: " + ", ".join(f"'{t}' h={h}" for t,h in out))

for name, html in CASES.items():
    run(name, html)
