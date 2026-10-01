"""What-if for linkedin's layered variant: resolve the hero's `display: revert-layer` by hand (to
the atoms layer's `display: grid`), render with a binary, and save the frame as a PNG.
usage: python3 whatif_revert.py <bin>"""
import os, re, subprocess, sys
from PIL import Image
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite'
s = open(f'{HUB}/scratch/li0930/li-new.html', errors='replace').read()
print('revert-layer occurrences', s.count('revert-layer'))
for m in list(re.finditer(r'[^{};]*\{\s*[\w-]+\s*:\s*revert-layer', s))[:12]:
    print('  ', ' '.join(m.group(0).split())[-90:])
s2, n = re.subn(r'(\.auya83\s*\{\s*display:\s*)revert-layer', r'\1grid', s)
print('replaced', n)
page = f'{HUB}/scratch/s0930e/li-whatif-revert.html'
open(page, 'w').write(s2)
out = f'{HUB}/scratch/s0930e/li-whatif-revert'
r = subprocess.run([sys.argv[1], '--html-file', page, '--dump-frame', out + '.ppm'], capture_output=True, text=True, timeout=300)
print('rc', r.returncode)
Image.open(out + '.ppm').save(out + '.png')
