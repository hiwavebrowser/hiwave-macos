#!/usr/bin/env python3
"""PAINT-0 P0c comparator: attribute the flat-vs-metrics delta.

Usage: paint0_compare.py <flat.log> <metrics.log>

Joins the two probe logs and reports:
  1. atlas: do the glyph bitmap hashes match? (identical => any pixel delta
     is pure seating, never raster)
  2. layout: per-line half_leading / y_cmd deltas, and how many y_cmd values
     are integral in each build
  3. paint: baseline / glyph_y deltas for the sampled chars, fractional-part
     histogram of baselines (the AA-relevant quantity)

Verdict line at the end: seating-only / raster-diff / mixed.
"""
import re
import sys
from collections import defaultdict

def parse(path):
    layout, paint, glyphs, atlas = [], [], [], {}
    for line in open(path):
        line = line.strip()
        if line.startswith("PAINT0 layout "):
            d = dict(re.findall(r"(\w+)=({[^}]*}|\"[^\"]*\"|\S+)", line[14:]))
            layout.append(d)
        elif line.startswith("PAINT0 paint "):
            d = dict(re.findall(r"(\w+)=(\"[^\"]*\"|\S+)", line[13:]))
            paint.append(d)
        elif line.startswith("PAINT0 glyph "):
            d = dict(re.findall(r"(\w+)=('[^']*'|\S+)", line[13:]))
            glyphs.append(d)
        elif line.startswith("PAINT0 atlas "):
            d = dict(re.findall(r"(\w+)=('[^']*'|\S+)", line[13:]))
            atlas[(d["cp"], d["fs"])] = d
    return layout, paint, glyphs, atlas

def frac(x):
    return x - int(x)

def fmean(vals):
    return sum(vals) / len(vals) if vals else 0.0

def main():
    fl_layout, fl_paint, fl_glyph, fl_atlas = parse(sys.argv[1])
    me_layout, me_paint, me_glyph, me_atlas = parse(sys.argv[2])

    print("== 1. atlas bitmap A/B ==")
    common = set(fl_atlas) & set(me_atlas)
    mismatch = [k for k in common if fl_atlas[k]["hash"] != me_atlas[k]["hash"]]
    only_f = len(set(fl_atlas) - set(me_atlas))
    only_m = len(set(me_atlas) - set(fl_atlas))
    print(f"common glyphs={len(common)} hash_mismatch={len(mismatch)} "
          f"flat_only={only_f} metrics_only={only_m}")
    for k in mismatch[:10]:
        print(f"  MISMATCH cp={k[0]} fs={k[1]} flat={fl_atlas[k]['hash']} metrics={me_atlas[k]['hash']}")

    print("\n== 2. layout seating (line-by-line join) ==")
    n = min(len(fl_layout), len(me_layout))
    print(f"layout lines: flat={len(fl_layout)} metrics={len(me_layout)} joined={n}")
    dh, dy = [], []
    int_y_f = int_y_m = 0
    for a, b in zip(fl_layout[:n], me_layout[:n]):
        if a["text"] != b["text"]:
            continue
        dh.append(float(b["half"]) - float(a["half"]))
        dy.append(float(b["y_cmd"]) - float(a["y_cmd"]))
        if abs(frac(float(a["y_cmd"]))) < 1e-4: int_y_f += 1
        if abs(frac(float(b["y_cmd"]))) < 1e-4: int_y_m += 1
    if dh:
        print(f"mean d_half_leading={fmean(dh):+.4f} max={max(dh, key=abs):+.4f}")
        print(f"mean d_y_cmd={fmean(dy):+.4f} max={max(dy, key=abs):+.4f}")
        print(f"integral y_cmd: flat {int_y_f}/{len(dh)} metrics {int_y_m}/{len(dh)}")

    print("\n== 3. paint seating (baseline fractional parts) ==")
    for name, paints in (("flat", fl_paint), ("metrics", me_paint)):
        fracs = [abs(frac(float(p["baseline"]))) for p in paints]
        integral = sum(1 for f in fracs if f < 1e-4 or f > 1 - 1e-4)
        print(f"{name}: runs={len(paints)} integral_baselines={integral} "
              f"mean_frac={fmean(fracs):.4f}")
    n2 = min(len(fl_glyph), len(me_glyph))
    dg = []
    for a, b in zip(fl_glyph[:n2], me_glyph[:n2]):
        if a["ch"] == b["ch"] and a["fs"] == b["fs"]:
            dg.append(float(b["glyph_y"]) - float(a["glyph_y"]))
    if dg:
        print(f"sampled glyphs joined={len(dg)} mean d_glyph_y={fmean(dg):+.4f} "
              f"max={max(dg, key=abs):+.4f}")
        nonzero = sum(1 for d in dg if abs(d) > 1e-3)
        print(f"glyphs that moved={nonzero}/{len(dg)}")

    print("\n== verdict ==")
    if common and not mismatch:
        if dg and any(abs(d) > 1e-3 for d in dg):
            print("SEATING-ONLY: bitmaps identical, glyph_y moved -> pixel delta is pure seating.")
        else:
            print("NEITHER moved: bitmaps identical and seating identical -> delta is elsewhere.")
    elif mismatch:
        print("RASTER-DIFF (or MIXED): glyph bitmaps differ between builds.")
    else:
        print("NO COMMON GLYPHS: instrument problem, rerun.")

if __name__ == "__main__":
    main()
