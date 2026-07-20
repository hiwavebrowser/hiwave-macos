#!/usr/bin/env python3
"""P1/P2 probe for the form-controls t8 dig (2026-07-17-form-controls-t8-DIG-IMPLEMENT §4.0).

Joins the RustKit layout dump (websuite/micro/form-controls/index.layout.json,
captured at 800x1200) against Chrome CfT-148 layout-rects by document order:
RK materializes one box per element except <option> (skipped by Chrome-side
filter) and includes an <html> root Chrome's dump omits (skipped RK-side).

Usage: python3 parity-tests/probe/probe_form_controls_join.py
"""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
RK_PATH = ROOT / "websuite/micro/form-controls/index.layout.json"
CH_PATH = ROOT / "baselines/chrome-148/micro/form-controls/layout-rects.json"


def flatten(node, out):
    if node.get("type") == "text":
        return
    out.append(node)
    for c in node.get("children", []):
        flatten(c, out)


def main():
    rk = json.load(open(RK_PATH))
    ch = json.load(open(CH_PATH))

    rk_boxes = []
    flatten(rk["root"], rk_boxes)
    rk_boxes = rk_boxes[1:]  # drop <html>; Chrome dump starts at body

    ch_elems = [e for e in ch["elements"] if e["tag"] != "option"]

    assert len(rk_boxes) == len(ch_elems), (len(rk_boxes), len(ch_elems))

    rows = []
    for i, (r, c) in enumerate(zip(rk_boxes, ch_elems)):
        bb = r["border_box"] if "border_box" in r else r["rect"]
        cr = c["rect"]
        rows.append({
            "i": i,
            "sel": c["selector"][:48],
            "tag": c["tag"],
            "rk": (round(bb["x"], 1), round(bb["y"], 1), round(bb["width"], 1), round(bb["height"], 1)),
            "ch": (cr["x"], cr["y"], cr["width"], cr["height"]),
            "dh": round(bb["height"] - cr["height"], 2),
            "dy": round(bb["y"] - cr["y"], 2),
            "dw": round(bb["width"] - cr["width"], 2),
        })

    print(f"{'sel':50} {'tag':9} {'RK h':>7} {'Ch h':>6} {'dh':>7} {'dy':>8} {'dw':>8}")
    for row in rows:
        print(f"{row['sel']:50} {row['tag']:9} {row['rk'][3]:7.1f} {row['ch'][3]:6.1f} "
              f"{row['dh']:7.2f} {row['dy']:8.2f} {row['dw']:8.2f}")

    print("\n=== P2: top 15 by |dh| then |dy| ===")
    for row in sorted(rows, key=lambda r: (-abs(r["dh"]), -abs(r["dy"])))[:15]:
        print(f"{row['sel']:50} {row['tag']:9} dh={row['dh']:7.2f} dy={row['dy']:8.2f} "
              f"rk={row['rk']} ch={row['ch']}")


if __name__ == "__main__":
    main()
