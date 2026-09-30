"""Pinned Chrome 148 rects for the empty-block probe pages (same HTML as the engine probe test)."""
import json, os, subprocess
HERE = os.path.dirname(os.path.abspath(__file__))
CHROME = os.path.expanduser('~/Repos/hiwave/hiwave-macos/.browsers/chrome/mac_arm-148.0.7778.216/chrome-mac-arm64/'
                            'Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing')
H = 'id="h" style="margin:8px 0;height:2px"'
PAGES = {
    'flex': f'<body><div id="o" style="display:flex"></div><div {H}></div></body>',
    'col': f'<body><div id="o" style="display:flex;flex-direction:column"></div><div {H}></div></body>',
    'grid': f'<body><div id="o" style="display:grid"></div><div {H}></div></body>',
    'ofh': f'<body><div id="o" style="overflow:hidden"></div><div {H}></div></body>',
    'plain': f'<body><div id="o"></div><div {H}></div></body>',
    'hr': '<body><div id="o" style="display:flex;flex-direction:column"></div><hr id="h"></body>',
    'mid': f'<body><div style="height:4px"></div><div id="o" style="display:flex;margin:5px 0"></div><div {H}></div></body>',
    'dt-hr': '<!doctype html><body><div id="o" style="display:flex;flex-direction:column"></div><hr id="h"></body>',
    'marg': '<!doctype html><body><div style="height:4px"></div><div id="o" style="margin-top:30px"></div><div id="h" style="height:2px"></div></body>',
    'spacer': '<!doctype html><body style="margin:0"><div style="display:flex;width:300px"><div id="o" style="flex:1"></div><div id="h" style="width:100px;height:2px"></div></div></body>',
}
d = os.path.join(HERE, 'probe')
os.makedirs(d, exist_ok=True)
for name, html in PAGES.items():
    p = os.path.join(d, name + '.html')
    open(p, 'w').write(html)
    subprocess.run(['node', os.path.join(HERE, '../../tools/parity_oracle/rects_local.mjs'), p, p + '.chrome.json'],
                   env=dict(os.environ, PARITY_CHROME_PATH=CHROME), check=True, timeout=180)
    r = json.load(open(p + '.chrome.json'))
    print(f"{name:7} o={r['o'][:4]} h={r['h'][:4]}")
