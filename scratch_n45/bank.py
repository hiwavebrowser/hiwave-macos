#!/usr/bin/env python3
"""bank.py LABEL CASE [CASE...] — copy the board results json and the named case captures under scratch_n45/."""
import shutil
import sys
from pathlib import Path

label = sys.argv[1]
cases = sys.argv[2:]
out = Path('scratch_n45')
shutil.copy('parity-baseline/parity_test_results.json', out / f'board_{label}.json')
dst = out / f'captures_{label}'
dst.mkdir(exist_ok=True)
for c in cases:
    src = Path('parity-baseline/captures') / c
    if src.exists():
        shutil.copytree(src, dst / c, dirs_exist_ok=True)
        print('banked', c)
    else:
        print('missing', src)
print('wrote', out / f'board_{label}.json')
