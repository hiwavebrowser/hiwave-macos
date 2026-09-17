#!/usr/bin/env python3
"""census.py — which board cases carry a transform with a scale/rotate/skew/matrix term (transform-origin matters)."""
import json
import pathlib
import re

reg = json.load(open('cases/registry.json'))
pat = re.compile(r'transform\s*:\s*([^;}]+)')
for name, c in reg.items():
    src = c.get('source') or c.get('html') or c.get('path') or ''
    p = pathlib.Path(src) if src else None
    if p is None or not p.exists():
        print(f'{name}: no source ({src}) keys={list(c)[:8]}')
        continue
    text = p.read_text(errors='replace')
    hits = [m.group(1).strip() for m in pat.finditer(text)]
    risky = [h for h in hits if re.search(r'scale|rotate|skew|matrix', h)]
    if hits:
        print(f'{name}: {len(hits)} transform decls, {len(risky)} scale/rotate/skew/matrix: {risky[:6]}')
