"""Chrome rects (pinned 148, by id) and a RustKit layout dump for the L0 fixture.
usage: l0_shots.py <binary> <label>   -> l0/chrome.json (kept if present), l0/<label>.layout.json"""
import json, os, subprocess, sys
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite'
D = f'{HUB}/scratch/s1001h/l0'
CHROME = os.path.expanduser('~/Repos/hiwave/hiwave-macos/.browsers/chrome/mac_arm-148.0.7778.216/chrome-mac-arm64/'
                            'Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing')
html = f'{D}/l0-grid-flex.html'
binary, label = sys.argv[1:3]
if not os.path.exists(f'{D}/chrome.json') or os.path.getmtime(f'{D}/chrome.json') < os.path.getmtime(html):
    subprocess.run(['node', f'{HUB}/tools/parity_oracle/shot_local.mjs', html, f'{D}/chrome.png', f'{D}/chrome.json',
                    'display'], env=dict(os.environ, PARITY_CHROME_PATH=CHROME), check=True, timeout=180, cwd=HUB)
r = subprocess.run([binary, '--html-file', html, '--width', '1280', '--height', '800',
                    '--dump-frame', f'{D}/{label}.ppm', '--dump-layout', f'{D}/{label}.layout.json'],
                   capture_output=True, text=True, timeout=180)
print('rustkit rc', r.returncode, r.stderr[-600:] if r.returncode else '')
c = json.load(open(f'{D}/chrome.json'))
for k, v in c.items():
    print(f'chrome {k:8}', [round(x, 2) for x in v['rect']])
s = open(f'{D}/{label}.layout.json').read()
print(len(s), 'bytes of layout;', s[:1500])
