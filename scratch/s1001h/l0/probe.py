"""Chrome rects by id and RustKit's form-control / leaf boxes for one local page, side by side in document order.
usage: probe.py <binary> <page.html> [props]   (Chrome json cached beside the page)"""
import json, os, subprocess, sys
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite'
CHROME = os.path.expanduser('~/Repos/hiwave/hiwave-macos/.browsers/chrome/mac_arm-148.0.7778.216/chrome-mac-arm64/'
                            'Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing')
binary, html = sys.argv[1], os.path.abspath(sys.argv[2])
props = sys.argv[3] if len(sys.argv) > 3 else 'font-family,font-size,line-height,padding,border-width,box-sizing'
stem = html[:-5]
cj = stem + '.chrome.json'
if not os.path.exists(cj) or os.path.getmtime(cj) < os.path.getmtime(html):
    subprocess.run(['node', f'{HUB}/tools/parity_oracle/shot_local.mjs', html, stem + '.chrome.png', cj, props],
                   env=dict(os.environ, PARITY_CHROME_PATH=CHROME), check=True, timeout=180, cwd=HUB)
r = subprocess.run([binary, '--html-file', html, '--width', '1280', '--height', '800',
                    '--dump-frame', stem + '.rk.ppm', '--dump-layout', stem + '.rk.layout.json'],
                   capture_output=True, text=True, timeout=180)
if r.returncode:
    print('rustkit rc', r.returncode, r.stderr[-800:])
c = json.load(open(cj))
for k, v in c.items():
    print(f'chrome  {k:8}', [round(x, 2) for x in v['rect']], {p: v[p] for p in v if p != 'rect'})


def walk(n):
    yield n
    for ch in n.get('children', []):
        yield from walk(ch)


for n in walk(json.load(open(stem + '.rk.layout.json'))['root']):
    if n.get('type') == 'form_control':
        b = n['border_box']
        print('rustkit control', [round(b[k], 2) for k in ('x', 'y', 'width', 'height')])
