#!/usr/bin/env python3
"""y_table.py — per-element y-table: Chrome layout-rects.json vs RustKit layout.json.

Pairs elements in document order (Chrome capture order == querySelectorAll('*')
document order; RustKit tree flattened depth-first, element boxes only, root
skipped). Prints the first divergence prominently.

Usage: python3 parity-tests/repro/y_table.py <chrome-layout-rects.json> <rustkit-layout.json> [tol]
"""
import json
import sys


def flatten(node, out):
    if node.get("type") == "text":
        return
    out.append(node)
    for c in node.get("children", []):
        flatten(c, out)


def main():
    chrome = json.load(open(sys.argv[1]))["elements"]
    rk_root = json.load(open(sys.argv[2]))["root"]
    tol = float(sys.argv[3]) if len(sys.argv) > 3 else 1.0

    rk = []
    flatten(rk_root, rk)
    # RustKit root wraps html; Chrome list starts at body. Drop RustKit
    # nodes until the count difference is absorbed from the front.
    while len(rk) > len(chrome):
        rk.pop(0)

    first_div = None
    print(f"{'selector':<58} {'chrome y':>9} {'rk y':>9} {'dy':>7}   {'chrome x':>9} {'rk x':>9} {'dx':>7}  {'ch h':>7} {'rk h':>7}")
    for i, (ce, re) in enumerate(zip(chrome, rk)):
        cr = ce["rect"]
        rr = re.get("border_box", {})
        dy = rr.get("y", 0) - cr["y"]
        dx = rr.get("x", 0) - cr["x"]
        dh = rr.get("height", 0) - cr["height"]
        flag = ""
        if abs(dy) > tol or abs(dx) > tol:
            flag = " <<<"
            if first_div is None:
                first_div = (i, ce["selector"], dy, dx)
        sel = ce["selector"][-58:]
        print(f"{sel:<58} {cr['y']:>9.1f} {rr.get('y',0):>9.1f} {dy:>7.1f}   {cr['x']:>9.1f} {rr.get('x',0):>9.1f} {dx:>7.1f}  {cr['height']:>7.1f} {rr.get('height',0):>7.1f}{flag}")

    if len(chrome) != len(rk):
        print(f"\nCOUNT MISMATCH: chrome={len(chrome)} rustkit={len(rk)} (alignment may drift)")
    if first_div:
        print(f"\nFIRST DIVERGENCE: idx {first_div[0]} {first_div[1]}  dy={first_div[2]:.1f} dx={first_div[3]:.1f}")


if __name__ == "__main__":
    main()
