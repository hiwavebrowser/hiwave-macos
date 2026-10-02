"""Per-box share of pixels that differ from pinned Chrome 148 (any channel by more than 16), for a local
page whose boxes have ids. Serves the page's directory over HTTP for RustKit when --serve is given.
usage: box_cmp.py <page.html> <label=binary>..."""
import json, os, subprocess, sys
from PIL import Image, ImageChops
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite'
CHROME = os.path.expanduser('~/Repos/hiwave/hiwave-macos/.browsers/chrome/mac_arm-148.0.7778.216/chrome-mac-arm64/'
                            'Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing')
html = os.path.abspath(sys.argv[1])
stem = html[:-5]
cj = stem + '.chrome.json'
if not os.path.exists(cj) or os.path.getmtime(cj) < os.path.getmtime(html):
    subprocess.run(['node', f'{HUB}/tools/parity_oracle/shot_local.mjs', html, stem + '.chrome.png', cj,
                    'background-color,background-repeat,background-position,background-size'],
                   env=dict(os.environ, PARITY_CHROME_PATH=CHROME), check=True, timeout=180, cwd=HUB)
c = json.load(open(cj))
chrome = Image.open(stem + '.chrome.png').convert('RGB')
if chrome.width != 1280:
    chrome = chrome.resize((1280, chrome.height * 1280 // chrome.width), Image.BOX)
import functools, http.server, threading


class H(http.server.SimpleHTTPRequestHandler):
    def log_message(self, fmt, *a):
        pass


srv = http.server.ThreadingHTTPServer(('127.0.0.1', 0), functools.partial(H, directory=os.path.dirname(html)))
threading.Thread(target=srv.serve_forever, daemon=True).start()
url = f'http://127.0.0.1:{srv.server_address[1]}/{os.path.basename(html)}'
frames = {}
for arg in sys.argv[2:]:
    label, binary = arg.split('=')
    out = f'{stem}.{label}.ppm'
    r = subprocess.run([binary, '--url', url, '--width', '1280', '--height', '800', '--dump-frame', out],
                       capture_output=True, text=True, timeout=180)
    if r.returncode:
        print(label, 'rc', r.returncode, r.stderr[-600:])
    frames[label] = Image.open(out).convert('RGB')
style = open(html).read()
off = {l: 0 for l in frames}
for k, v in c.items():
    x, y, w, h = [int(round(t)) for t in v['rect']]
    box = (x, y, x + w, y + h)
    row = [f'{k:4}']
    for l, f in frames.items():
        d = ImageChops.difference(chrome.crop(box), f.crop(box)).point(lambda p: 255 if p > 16 else 0).convert('L')
        n = sum(1 for p in d.getdata() if p)
        pct = 100.0 * n / (w * h)
        off[l] += 1 if pct > 1.0 else 0
        row.append(f'{l} {pct:6.2f}%')
    rule = [ln.strip() for ln in style.splitlines() if ln.startswith(f'#{k} ')]
    print('  '.join(row), ' | ', ' '.join(rule))
print('boxes more than 1% off Chrome:', ', '.join(f'{l} {off[l]} of {len(c)}' for l in frames))
