#!/usr/bin/env python3
"""chrome_repro.py HTML OUTDIR W H — capture the pinned-Chrome baseline (png + layout rects) for a repro,
the same captureBaseline() generate_baselines.py uses."""
import pathlib
import subprocess
import sys

html = pathlib.Path(sys.argv[1]).resolve()
out = pathlib.Path(sys.argv[2]).resolve()
w, h = sys.argv[3], sys.argv[4]
repo = pathlib.Path(__file__).resolve().parent.parent
script = (
    "import { captureBaseline } from './capture_baseline.mjs';\n"
    f"const r = await captureBaseline('{html}', '{out}', {w}, {h});\n"
    "console.log(JSON.stringify(r));\n"
)
r = subprocess.run(['node', '-e', script], capture_output=True, text=True, timeout=120,
                   cwd=repo / 'tools' / 'parity_oracle')
print(r.stdout[-600:])
print(r.stderr[-600:], file=sys.stderr)
sys.exit(r.returncode)
