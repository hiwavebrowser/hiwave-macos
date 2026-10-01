"""Pill probes: a wrapping flex row with `height: fit-content` (linkedin's topic pills).
Writes scratch/s0930e/probe/p-*.html (ids w = the wrap container, c = first pill, after) and
renders each with a binary.  usage: python3 mk2.py <bin> | python3 mk2.py - (write only)"""
import json, os, subprocess, sys
D = os.path.dirname(os.path.abspath(__file__)) + '/probe'
PILL = '<a class="pill" {id}><span>Career</span></a>'
BASE = '''<!doctype html><html><head><style>
body {{ margin: 0 }}
.outer {{ {outer} }}
#w {{ display: flex; flex-direction: row; flex-wrap: wrap; gap: 8px; {w} }}
.pill {{ display: inline-flex; align-items: center; min-height: 32px; box-sizing: border-box; background: #ccc }}
#after {{ background: #00f; height: 20px }}
</style></head><body>
<div class="outer"><div id="w">{pills}</div></div>
<div id="after"></div>
</body></html>'''
CASES = {
    'p-fit-block': ('', 'height: fit-content'),
    'p-auto-block': ('', ''),
    'p-fit-grid1fr': ('display: grid; grid-template-rows: 1fr', 'height: fit-content'),
    'p-auto-grid1fr': ('display: grid; grid-template-rows: 1fr', ''),
    'p-fit-narrow': ('width: 200px', 'height: fit-content'),
    'p-fit-in-row': ('display: flex', 'height: fit-content; width: 200px'),
    'p-fit-in-col': ('display: flex; flex-direction: column', 'height: fit-content; width: 200px'),
}


def find(nd, ident):
    sel = nd.get('selector') or ''
    if sel.endswith('#' + ident):
        return nd
    for c in nd.get('children', []):
        r = find(c, ident)
        if r:
            return r


for name, (outer, w) in CASES.items():
    pills = PILL.format(id='id="c"') + ''.join(PILL.format(id='') for _ in range(5))
    path = f'{D}/{name}.html'
    open(path, 'w').write(BASE.format(outer=outer, w=w, pills=pills))
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
    print(f'{name:20s}', row)
