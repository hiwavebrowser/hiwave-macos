"""Rects of every id'd element: pinned Chrome 148 vs a RustKit parity-capture binary.
usage: python3 ab.py <binary> <dir> [page-name ...]   pages come from PAGES below; writes <dir>/<name>.html"""
import json, os, re, subprocess, sys
HERE = os.path.dirname(os.path.abspath(__file__))
CHROME = os.path.expanduser('~/Repos/hiwave/hiwave-macos/.browsers/chrome/mac_arm-148.0.7778.216/chrome-mac-arm64/'
                            'Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing')
TRIM = ('<style>.t{display:block;font-size:17px;line-height:22px;font-family:Arial}'
        '.t:before{content:"";display:block;height:0;margin-top:-5px}'
        '.t:after{content:"";display:block;height:0;margin-bottom:-5px}</style>')
PAGES = {
    # facebook's "Log into Facebook" heading: leading trim with negative-margin pseudo blocks
    'trim': f'<!doctype html>{TRIM}<body style="margin:0"><span id="a" class="t">Log into Facebook</span><div id="b" style="height:2px"></div></body>',
    'trim-div': f'<!doctype html>{TRIM}<body style="margin:0"><div id="a" class="t">Log into Facebook</div><div id="b" style="height:2px"></div></body>',
    'nopseudo': '<!doctype html><body style="margin:0"><span id="a" style="display:block;font-size:17px;line-height:22px;font-family:Arial">Log into Facebook</span><div id="b" style="height:2px"></div></body>',
    'var-lh': '<!doctype html><body style="margin:0;--h:1.2941"><span id="a" style="display:block;font-size:17px;--x:calc(var(--h) * 1em);line-height:var(--x);font-family:Arial">Log into Facebook</span><div id="b" style="height:2px"></div></body>',
    'preline': '<!doctype html><body style="margin:0"><span id="a" style="display:block;font-size:17px;line-height:22px;white-space:pre-line;font-family:Arial">Log into Facebook</span><div id="b" style="height:2px"></div></body>',
    # an inline pseudo with text: its line should stay one line-height (22) tall
    'ipseudo': '<!doctype html><style>#a{font-size:17px;line-height:22px;font-family:Arial}#a:before{content:"> "}</style><body style="margin:0"><div id="a">text</div><div id="b" style="height:2px"></div></body>',
    'ipseudo-after': '<!doctype html><style>#a{font-size:17px;line-height:22px;font-family:Arial}#a:after{content:" <"}</style><body style="margin:0"><div id="a">text</div><div id="b" style="height:2px"></div></body>',
    'ipseudo-only': '<!doctype html><style>#a{font-size:17px;line-height:22px;font-family:Arial}#a:before{content:"x"}</style><body style="margin:0"><div id="a"></div><div id="b" style="height:2px"></div></body>',
    'ipseudo-lh': '<!doctype html><style>#a{font-size:17px;line-height:22px;font-family:Arial}#a:before{content:"> ";line-height:22px}</style><body style="margin:0"><div id="a">text</div><div id="b" style="height:2px"></div></body>',
    'ipseudo-span': '<!doctype html><style>#a{font-size:17px;line-height:22px;font-family:Arial}</style><body style="margin:0"><div id="a"><span>&gt; </span>text</div><div id="b" style="height:2px"></div></body>',
    'col': f'<!doctype html>{TRIM}<body style="margin:0"><div style="display:flex;flex-direction:column"><div id="w" style="display:flex;flex-direction:column"><span id="a" class="t">Log into Facebook</span></div></div><div id="b" style="height:2px"></div></body>',
}
binary, d = sys.argv[1], sys.argv[2]
names = sys.argv[3:] or list(PAGES)
os.makedirs(d, exist_ok=True)


def rk_rects(path):
    out = path + '.rk.json'
    subprocess.run([binary, '--html-file', path, '--dump-layout', out], capture_output=True, timeout=120)
    L = json.load(open(out))
    res = {}

    def walk(nd):
        m = re.search(r'#([\w-]+)$', nd.get('selector') or '')
        if m and nd.get('tag'):
            b = nd['border_box']
            res[m.group(1)] = [round(b['x'], 1), round(b['y'], 1), round(b['width'], 1), round(b['height'], 1)]
        for c in nd.get('children', []):
            walk(c)
    walk(L['root'])
    return res


for name in names:
    p = os.path.join(d, name + '.html')
    open(p, 'w').write(PAGES[name])
    subprocess.run(['node', os.path.join(HERE, '../../tools/parity_oracle/rects_local.mjs'), p, p + '.chrome.json'],
                   env=dict(os.environ, PARITY_CHROME_PATH=CHROME), check=True, timeout=180, capture_output=True)
    c = json.load(open(p + '.chrome.json'))
    r = rk_rects(p)
    for i in sorted(set(c) | set(r)):
        cc = [round(v, 1) for v in c[i][:4]] if i in c else None
        flag = '' if cc and i in r and all(abs(x - y) <= 1 for x, y in zip(cc, r[i])) else '  <--'
        print(f'{name:9} {i}: chrome {cc}  rk {r.get(i)}{flag}')
