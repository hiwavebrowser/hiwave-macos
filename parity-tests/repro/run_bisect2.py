#!/usr/bin/env python3
"""Run all bisect2 variants and report positioning_rate (0.947=OK, 0.789=broken)."""
import json
import subprocess

for name in ['font', 'size', 'lh', 'color', 'bg', 'smooth']:
    path = 'parity-tests/repro/bisect2-' + name + '.html'
    stem = path.rsplit('.', 1)[0]
    cmd = [
        './target/release/parity-capture',
        '--html-file', path,
        '--width', '600', '--height', '500',
        '--dump-layout', stem + '.layout.json',
    ]
    r = subprocess.run(cmd, capture_output=True, text=True, timeout=120)
    try:
        out = json.loads(r.stdout.strip().splitlines()[-1])
        rate = out['layout_stats']['positioning_rate']
        verdict = 'OK' if rate > 0.9 else 'BROKEN'
        print(name, rate, verdict)
    except Exception as e:
        print(name, 'ERROR', e, r.stdout[-200:], r.stderr[-200:])
