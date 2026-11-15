"""CB3c: build the button-width matrix page (run from the repo root).

Each cell is a <button> with a label and a style; the page records its border-box width and height
as a ';;' separated list, written into #out, so Chrome's --dump-dom (with a virtual time budget)
carries the numbers out. The same page is loaded by RustKit in cb3c_button_widths_run.py.
"""
import json, os, sys

LABELS = {"x": "x", "Save": "Save", "Medium Button": "Medium Button"}
# (id, inline style). font-size is explicit where the UA control font would make rounding noisy.
STYLES = [
    ("bare", ""),
    ("bare16", "font-size:16px"),
    ("p0", "font-size:16px;padding:0"),
    ("p04", "font-size:16px;padding:0 4px"),
    ("p816", "font-size:16px;padding:8px 16px"),
    ("bn", "font-size:16px;border:none"),
    ("p0bn", "font-size:16px;padding:0;border:none"),
    ("p04bn", "font-size:16px;padding:0 4px;border:none"),
    ("lh1", "font-size:16px;line-height:1"),
    ("pl0", "font-size:16px;padding-left:0"),
    ("pr20", "font-size:16px;padding-right:20px"),
    ("pv0", "font-size:16px;padding-top:0;padding-bottom:0"),
    ("bw0", "font-size:16px;border-width:0"),
    ("bsolid", "font-size:16px;border-style:solid"),
    ("minw", "font-size:16px;min-width:100px"),
    ("w120", "font-size:16px;width:120px"),
    ("wmax", "font-size:16px;width:max-content"),
]

def cells():
    for label_id, label in LABELS.items():
        for style_id, style in STYLES:
            yield f"{style_id}|{label_id}", label, style

def page():
    rows = []
    for cid, label, style in cells():
        rows.append(f'<div><button id="{cid}" style="{style}">{label}</button></div>')
    script = (
        "var ids=" + json.dumps([c[0] for c in cells()]) + ";"
        "var out=ids.map(function(i){var r=document.getElementById(i).getBoundingClientRect();"
        "return i+'='+r.width+'x'+r.height;}).join(';;');"
        "document.getElementById('out').textContent=out;"
    )
    return ("<!doctype html><html><head><meta charset=utf-8><style>body{margin:0;font:16px Arial}div{margin:4px 8px}"
            "#out{display:none}</style></head><body>" + "".join(rows) + "<pre id=out></pre><script>" + script + "</script></body></html>")

if __name__ == "__main__":
    out = sys.argv[1] if len(sys.argv) > 1 else "docs/diagnostics/2026-11-11/cb3c_button_widths.html"
    with open(out, "w", encoding="utf-8") as f:
        f.write(page())
    print("wrote", out, len(list(cells())), "cells")
