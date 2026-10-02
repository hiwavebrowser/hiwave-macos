"""Chrome rects by id against RustKit boxes for a local page. The layout dump has no ids, so each Chrome
rect is paired with the RustKit box nearest to it (x, y, w, h distance), and the pairing distance is shown.
usage: probe2.py <binary> <page.html> <label>"""
import json, os, subprocess, sys
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite'
CHROME = os.path.expanduser('~/Repos/hiwave/hiwave-macos/.browsers/chrome/mac_arm-148.0.7778.216/chrome-mac-arm64/'
                            'Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing')
binary, html, label = sys.argv[1], os.path.abspath(sys.argv[2]), sys.argv[3]
stem = html[:-5]
cj = stem + '.chrome.json'
if not os.path.exists(cj) or os.path.getmtime(cj) < os.path.getmtime(html):
    subprocess.run(['node', f'{HUB}/tools/parity_oracle/shot_local.mjs', html, stem + '.chrome.png', cj, 'display'],
                   env=dict(os.environ, PARITY_CHROME_PATH=CHROME), check=True, timeout=180, cwd=HUB)
lj = f'{stem}.{label}.layout.json'
r = subprocess.run([binary, '--html-file', html, '--width', '1280', '--height', '800',
                    '--dump-frame', f'{stem}.{label}.ppm', '--dump-layout', lj],
                   capture_output=True, text=True, timeout=180)
if r.returncode:
    print('rustkit rc', r.returncode, r.stderr[-800:])


def walk(n):
    yield n
    for ch in n.get('children', []):
        yield from walk(ch)


boxes = []
for n in walk(json.load(open(lj))['root']):
    if 'border_box' in n:
        b = n['border_box']
        boxes.append([b['x'], b['y'], b['width'], b['height']])
bad = 0
for k, v in json.load(open(cj)).items():
    cr = v['rect']
    best = min(boxes, key=lambda b: sum(abs(p - q) for p, q in zip(b, cr)))
    off = [a for a, p, q in zip('xywh', cr, best) if abs(p - q) > 0.5]
    bad += bool(off)
    print(f'{k:10} chrome {[round(x, 2) for x in cr]}  rustkit {[round(x, 2) for x in best]}  '
          f'{"OFF " + "".join(off) if off else "ok"}')
print(f'{label}: {bad} boxes off by more than 0.5px')
