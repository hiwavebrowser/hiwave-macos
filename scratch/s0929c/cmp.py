"""RustKit vs Chrome rects of every [id] element in a local page.
usage: python3 cmp.py <capture-bin> <page.html>"""
import json, os, re, subprocess, sys
HERE = os.path.dirname(os.path.abspath(__file__))
BIN, P = sys.argv[1], sys.argv[2]
CHROME = os.path.expanduser('~/Repos/hiwave/hiwave-macos/.browsers/chrome/mac_arm-148.0.7778.216/chrome-mac-arm64/'
                            'Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing')
cj, rj = P + '.chrome.json', P + '.rk.json'
subprocess.run(['node', os.path.join(HERE, '../../tools/parity_oracle/rects_local.mjs'), P, cj],
               env=dict(os.environ, PARITY_CHROME_PATH=CHROME), check=True, timeout=180)
subprocess.run([BIN, '--html-file', P, '--dump-layout', rj], capture_output=True, timeout=300)
C, L = json.load(open(cj)), json.load(open(rj))


def walk(nd):
    m = re.search(r'#([\w-]+)$', nd.get('selector') or '')
    if m and nd.get('tag'):
        b = nd['border_box']
        c = C.get(m.group(1), [0, 0, 0, 0, '?'])
        flag = '  <--' if abs(b['height'] - c[3]) > 1 or abs(b['y'] - c[1]) > 1 else ''
        print(f"{m.group(1):6} rk y={b['y']:.0f} h={b['height']:.0f} | ch y={c[1]:.0f} h={c[3]:.0f}{flag}")
    for c in nd.get('children', []):
        walk(c)


walk(L['root'])
