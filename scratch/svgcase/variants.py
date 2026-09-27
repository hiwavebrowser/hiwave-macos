"""Render dvh.html variants in RustKit; print the red box origin. usage: variants.py <bin>"""
import os, subprocess, sys
from PIL import Image
here = os.path.dirname(os.path.abspath(__file__))
src = open(os.path.join(here, 'dvh.html')).read()
variants = {
    'dvh': src,
    'vh': src.replace('100dvh', '100vh'),
    'px': src.replace('min-height:100dvh', 'min-height:1000px'),
    'height-px': src.replace('min-height:100dvh', 'height:1000px'),
    'no-minh-right': src.replace(';min-height:45vh', ''),
    'row-only-h': src.replace('<div id="outer" style="display:flex;flex-direction:column;min-height:100dvh">', '<div>')
                     .replace('display:flex;flex:1 1 0%">', 'display:flex;height:1000px">'),
    'right-only-h': '<!DOCTYPE html><html><body style="margin:0"><div style="display:flex;height:1000px;align-items:center;justify-content:center"><div style="width:50px;height:50px;background:red"></div></div></body></html>',
    'right-stretched': '<!DOCTYPE html><html><body style="margin:0"><div style="display:flex;height:1000px"><div style="display:flex;flex:1;align-items:center;justify-content:center"><div style="width:50px;height:50px;background:red"></div></div></div></body></html>',
    'col-stretched': '<!DOCTYPE html><html><body style="margin:0"><div style="display:flex;flex-direction:column;height:1000px"><div style="display:flex;flex:1;align-items:center;justify-content:center"><div style="width:50px;height:50px;background:red"></div></div></div></body></html>',
}
for name, html in variants.items():
    f = os.path.join(here, f'v-{name}.html')
    open(f, 'w').write(html)
    out = f[:-5] + '.ppm'
    p = subprocess.run([sys.argv[1], '--html-file', f, '--width', '400', '--height', '1000', '--dump-frame', out],
                       capture_output=True, text=True)
    im = Image.open(out).convert('RGB'); px = im.load()
    pts = [(x, y) for y in range(im.height) for x in range(im.width) if px[x, y][0] > 200 and px[x, y][1] < 60]
    print(name, (min(q[0] for q in pts), min(q[1] for q in pts)) if pts else 'none')
