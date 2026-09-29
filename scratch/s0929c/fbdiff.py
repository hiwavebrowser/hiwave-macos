"""Tag every body element with id=rkN, lay the page out in RustKit and pinned Chrome,
and print the first (outermost) elements whose heights diverge while their parent's
children... i.e. elements that differ but whose element children all match.

usage: python3 fbdiff.py <capture-bin> [src.html]
"""
import json, os, re, subprocess, sys

HERE = os.path.dirname(os.path.abspath(__file__))
BIN = sys.argv[1]
SRC = sys.argv[2] if len(sys.argv) > 2 else os.path.join(HERE, '../bing0929/fb-local.html')
TAG = os.path.join(HERE, 'fb-ids.html')
CHROME = os.path.expanduser('~/Repos/hiwave/hiwave-macos/.browsers/chrome/mac_arm-148.0.7778.216/chrome-mac-arm64/'
                            'Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing')

src = open(SRC, encoding='utf-8', errors='replace').read()
body = src.find('<body')
n = [0]


def tag(m):
    if re.search(r'\sid=', m.group(0)):
        return m.group(0)
    n[0] += 1
    return f'<{m.group(1)} id="rk{n[0]}"' + m.group(2)


tagged = src[:body] + re.sub(r'<([a-zA-Z][a-zA-Z0-9-]*)(\s|>|/)', tag, src[body:])
open(TAG, 'w').write(tagged)
print('tagged', n[0])

cj = os.path.join(HERE, 'fb-chrome.json')
rj = os.path.join(HERE, 'fb-rk.json')
subprocess.run(['node', os.path.join(HERE, '../../tools/parity_oracle/rects_local.mjs'), TAG, cj],
               env=dict(os.environ, PARITY_CHROME_PATH=CHROME), check=True, timeout=180)
subprocess.run([BIN, '--html-file', TAG, '--dump-layout', rj], capture_output=True, timeout=300)
C = json.load(open(cj))
L = json.load(open(rj))

# RustKit: id -> (rect, children ids); selector ends with '#rkN' hopefully
rk = {}


def ident(nd):
    s = nd.get('selector') or ''
    m = re.search(r'#([\w-]+)$', s)
    return m.group(1) if m else None


def walk(nd, parent):
    i = ident(nd) if nd.get('tag') else None
    me = parent
    if i:
        b = nd['border_box']
        rk[i] = {'r': (b['x'], b['y'], b['width'], b['height']), 'kids': [], 'parent': parent}
        if parent:
            rk[parent]['kids'].append(i)
        me = i
    for c in nd.get('children', []):
        walk(c, me)


walk(L['root'], None)
print('rk ids', len(rk), 'chrome ids', len(C))
if not rk:
    print('selector sample:', L['root']['children'][0]['children'][0].get('selector'))
    sys.exit(1)


def bad(i):
    c = C.get(i)
    if not c:
        return False
    r = rk[i]['r']
    return abs(r[3] - c[3]) > 2 or abs(r[1] - c[1]) > 2


roots = []
for i, v in rk.items():
    if bad(i) and not any(bad(k) for k in v['kids']):
        roots.append(i)
roots.sort(key=lambda i: int(i[2:]) if i.startswith('rk') else 0)
m = re.compile(r'<[^>]*\bid="%s"[^>]*>')
for i in roots[:40]:
    r = rk[i]['r']
    c = C[i]
    tagm = re.search(r'<[^>]*\bid="%s"[^>]*>' % re.escape(i), tagged)
    print(f"{i}: rk y={r[1]:.0f} h={r[3]:.0f} w={r[2]:.0f} | chrome y={c[1]:.0f} h={c[3]:.0f} w={c[2]:.0f} {c[4]}  {tagm.group(0)[:160] if tagm else ''}")
