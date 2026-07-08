#!/usr/bin/env python3
"""Run the same capture N times; report distinct body geometries (flake check)."""
import json
import subprocess
import sys
from collections import Counter

html = sys.argv[1] if len(sys.argv) > 1 else 'parity-tests/repro/bisect2-font.html'
n = int(sys.argv[2]) if len(sys.argv) > 2 else 10
geoms = Counter()
for i in range(n):
    cmd = [
        './target/release/parity-capture',
        '--html-file', html,
        '--width', '600', '--height', '500',
        '--dump-layout', '/tmp/flake.layout.json',
    ]
    subprocess.run(cmd, capture_output=True, text=True, timeout=120)
    d = json.load(open('/tmp/flake.layout.json'))
    body = d['root']['children'][0]
    r = body.get('content_rect') or {}
    geoms[(r.get('x'), r.get('y'), round(r.get('width', 0), 1))] += 1
for g, c in geoms.items():
    print('body(x,y,w) =', g, 'count', c)
