"""The painted colour at the centre of each box of bgsh.html: pinned Chrome 148 against each binary.
usage: bgsh_cmp.py <page.html> <label=binary>..."""
import json, os, subprocess, sys
from PIL import Image
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite'
CHROME = os.path.expanduser('~/Repos/hiwave/hiwave-macos/.browsers/chrome/mac_arm-148.0.7778.216/chrome-mac-arm64/'
                            'Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing')
html = os.path.abspath(sys.argv[1])
stem = html[:-5]
cj = stem + '.chrome.json'
if not os.path.exists(cj) or os.path.getmtime(cj) < os.path.getmtime(html):
    subprocess.run(['node', f'{HUB}/tools/parity_oracle/shot_local.mjs', html, stem + '.chrome.png', cj,
                    'background-color,background-image'],
                   env=dict(os.environ, PARITY_CHROME_PATH=CHROME), check=True, timeout=180, cwd=HUB)
c = json.load(open(cj))
frames = {'chrome': Image.open(stem + '.chrome.png').convert('RGB')}
for arg in sys.argv[2:]:
    label, binary = arg.split('=')
    out = f'{stem}.{label}.ppm'
    r = subprocess.run([binary, '--html-file', html, '--width', '1280', '--height', '800', '--dump-frame', out],
                       capture_output=True, text=True, timeout=180)
    if r.returncode:
        print(label, 'rc', r.returncode, r.stderr[-600:])
    frames[label] = Image.open(out).convert('RGB')
sx = frames['chrome'].width / 1280.0
labels = [k for k in frames if k != 'chrome']
wrong = {k: 0 for k in labels}
style = open(html).read()
for k, v in c.items():
    x, y, w, h = v['rect']
    cx, cy = x + w / 2, y + h / 2
    ch = frames['chrome'].getpixel((int(cx * sx), int(cy * sx)))
    row = [f'{k:4} chrome {str(ch):16} computed {v.get("background-color", "?"):18}']
    for l in labels:
        px = frames[l].getpixel((int(cx), int(cy)))
        ok = all(abs(a - b) <= 8 for a, b in zip(px, ch))
        wrong[l] += 0 if ok else 1
        row.append(f'{l} {str(px):16}{"" if ok else " WRONG"}')
    rule = [ln.strip() for ln in style.splitlines() if ln.startswith(f'#{k} ')]
    print('  '.join(row), ' | ', ' '.join(rule))
print('boxes whose painted colour differs from Chrome:', ', '.join(f'{l} {wrong[l]} of {len(c)}' for l in labels))
