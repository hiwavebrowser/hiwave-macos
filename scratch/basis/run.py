"""Render flex-basis variants; print red box origin. usage: run.py <bin>"""
import os, subprocess, sys
from PIL import Image
here = os.path.dirname(os.path.abspath(__file__))
def row(basis_l, basis_r, lh='height:100px;', inner='<div style="width:50px;height:50px;background:red"></div>', rstyle='display:flex;align-items:center;justify-content:center;'):
    return f'<!DOCTYPE html><html><body style="margin:0"><div style="display:flex;height:400px"><div style="flex:{basis_l};{lh}background:#ccc"></div><div style="{rstyle}flex:{basis_r}">{inner}</div></div></body></html>'
variants = {
  'pct': row('1 1 0%', '1 1 0%'),
  'zero': row('1 1 0', '1 1 0'),
  'one': row('1', '1'),
  'px0': row('1 1 0px', '1 1 0px'),
  'pct-block': row('1 1 0%', '1 1 0%', rstyle=''),
  'pct-noh': row('1 1 0%', '1 1 0%', lh=''),
  'longhand': row('1 1 0%', '1 1 0%').replace('flex:1 1 0%', 'flex-grow:1;flex-shrink:1;flex-basis:0%'),
}
for name, html in variants.items():
    f = os.path.join(here, f'b-{name}.html'); open(f, 'w').write(html)
    out = f[:-5] + '.ppm'
    subprocess.run([sys.argv[1], '--html-file', f, '--width', '400', '--height', '400', '--dump-frame', out], capture_output=True, text=True)
    im = Image.open(out).convert('RGB'); px = im.load()
    pts = [(x, y) for y in range(im.height) for x in range(im.width) if px[x, y][0] > 200 and px[x, y][1] < 60]
    print(name, (min(q[0] for q in pts), min(q[1] for q in pts)) if pts else 'none')
