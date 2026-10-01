"""white-space: pre-wrap probes (linkedin layered: headings in a 452px column come out 1025px wide).
Writes scratch/s0930e/probe/w-*.html (ids w = the 200px box/column, c = the paragraph, after) and
renders each with a binary.  usage: python3 mk3.py <bin> | python3 mk3.py - (write only)"""
import json, os, subprocess, sys
D = os.path.dirname(os.path.abspath(__file__)) + '/probe'
TEXT = 'Discover relevant posts and expert insights curated by topic and in one place.'
BASE = '''<!doctype html><html><head><style>
body {{ margin: 0; font: 16px/20px Arial }}
#w {{ width: 200px; {w} }}
#c {{ margin: 0; {c} }}
#after {{ background: #00f; height: 20px }}
</style></head><body>
<div id="w"><p id="c">{text}</p></div>
<div id="after"></div>
</body></html>'''
COL = 'display: flex; flex-direction: column'
CASES = {
    'w-normal-block': ('', ''),
    'w-prewrap-block': ('', 'white-space: pre-wrap'),
    'w-preline-block': ('', 'white-space: pre-line'),
    'w-normal-col': (COL, ''),
    'w-prewrap-col': (COL, 'white-space: pre-wrap'),
    'w-prewrap-breakword-col': (COL, 'white-space: pre-wrap; word-break: break-word'),
    'w-breakword-col': (COL, 'word-break: break-word'),
}


def find(nd, ident):
    sel = nd.get('selector') or ''
    if sel.endswith('#' + ident):
        return nd
    for c in nd.get('children', []):
        r = find(c, ident)
        if r:
            return r


for name, (w, c) in CASES.items():
    path = f'{D}/{name}.html'
    open(path, 'w').write(BASE.format(w=w, c=c, text=TEXT))
    if sys.argv[1] == '-':
        continue
    out = f'{D}/{name}-{os.path.basename(sys.argv[1])}.json'
    subprocess.run([sys.argv[1], '--html-file', path, '--dump-layout', out], capture_output=True, timeout=120)
    L = json.load(open(out))
    row = []
    for ident in ('w', 'c', 'after'):
        nd = find(L['root'], ident)
        b = nd['border_box'] if nd else None
        row.append((ident, [round(b[k], 1) for k in ('x', 'y', 'width', 'height')] if b else None))
    print(f'{name:24s}', row)
