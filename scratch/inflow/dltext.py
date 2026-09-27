#!/usr/bin/env python3
"""Print the text ops of RustKit display lists: dltext.py <dl.json> [<dl.json> ...]"""
import json, sys

for path in sys.argv[1:]:
    d = json.load(open(path))
    ops = d if isinstance(d, list) else d.get("commands") or d.get("ops") or d.get("items")
    print("==", path)
    for o in ops:
        if "text" not in o:
            continue
        pos = {k: o[k] for k in ("x", "y", "rect", "origin", "position") if k in o}
        print("  ", repr(o.get("text"))[:50], json.dumps(pos)[:120])
