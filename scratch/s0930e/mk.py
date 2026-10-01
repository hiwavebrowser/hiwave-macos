"""Write the hero-wrapper bisect probes (scratch/s0930e/probe/*.html) and render each with a binary,
printing the wrapper (#w) and the following sibling (#after) boxes.
usage: python3 mk.py <bin> [chrome]   (chrome: print Chrome 148's rects instead)"""
import json, os, subprocess, sys
D = os.path.dirname(os.path.abspath(__file__)) + '/probe'
os.makedirs(D, exist_ok=True)
BASE = '''<!doctype html><html><head><style>
body {{ margin: 0 }}
.p {{ display: flex; flex-direction: column; {parent} }}
#w {{ {w} }}
#w > * {{ {kid} }}
.col {{ display: flex; flex-direction: column }}
#after {{ background: #00f; height: 20px }}
</style></head><body>
<div class="p"><div id="w"><div class="col" id="c"><div style="height:40px;background:#0f0"></div><div style="height:40px;background:#f00"></div></div></div>
<div id="after"></div></div>
</body></html>'''
GRID = 'grid-area: 1/-1; min-width: 0'
CASES = {
    'a-all': ('', 'flex: 1; display: grid; height: fit-content; width: auto', GRID),
    'b-noflex': ('', 'display: grid; height: fit-content; width: auto', GRID),
    'c-nofit': ('', 'flex: 1; display: grid; width: auto', GRID),
    'd-nogrid': ('', 'flex: 1; height: fit-content', ''),
    'e-fitonly-block': ('', 'height: fit-content', ''),
    'f-flex1-block': ('', 'flex: 1', ''),
    'g-all-parent300': ('height: 300px', 'flex: 1; display: grid; height: fit-content; width: auto', GRID),
    'h-flex1-parent300': ('height: 300px', 'flex: 1', ''),
    'i-fit-plain': (None, 'height: fit-content', ''),
    'j-min-plain': (None, 'height: min-content', ''),
    'k-max-plain': (None, 'height: max-content', ''),
    'l-fit-grid-plain': (None, 'display: grid; height: fit-content', GRID),
}
JS = "JSON.stringify(['w','c','after'].map(i=>{const r=document.getElementById(i).getBoundingClientRect();return [i,r.x,r.y,r.width,r.height]}))"


def find(nd, ident):
    sel = nd.get('selector') or ''
    if sel.endswith('#' + ident) or sel == '#' + ident:
        return nd
    for c in nd.get('children', []):
        r = find(c, ident)
        if r:
            return r


for name, (parent, w, kid) in CASES.items():
    html = BASE.format(parent=parent or '', w=w, kid=kid)
    if parent is None:
        html = html.replace('display: flex; flex-direction: column; ', '')
    path = f'{D}/{name}.html'
    open(path, 'w').write(html)
    if len(sys.argv) > 2 and sys.argv[2] == 'chrome':
        continue
    out = f'{D}/{name}-{os.path.basename(sys.argv[1])}.json'
    subprocess.run([sys.argv[1], '--html-file', path, '--dump-layout', out], capture_output=True, timeout=120)
    L = json.load(open(out))
    row = []
    for ident in ('w', 'c', 'after'):
        nd = find(L['root'], ident)
        b = nd['border_box'] if nd else None
        row.append((ident, [round(b[k], 1) for k in ('x', 'y', 'width', 'height')] if b else None))
    print(f'{name:20s}', row)
