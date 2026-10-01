"""Pinned Chrome 148 rects (#w, #c, #after) for every probe page in scratch/s0930e/probe.
usage: python3 chrome.py [name-prefix]"""
import glob, json, os, subprocess, sys
HERE = os.path.dirname(os.path.abspath(__file__))
CHROME = os.path.expanduser('~/Repos/hiwave/hiwave-macos/.browsers/chrome/mac_arm-148.0.7778.216/chrome-mac-arm64/'
                            'Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing')
pre = sys.argv[1] if len(sys.argv) > 1 else ''
for p in sorted(glob.glob(f'{HERE}/probe/{pre}*.html')):
    subprocess.run(['node', os.path.join(HERE, '../../tools/parity_oracle/rects_local.mjs'), p, p + '.chrome.json'],
                   env=dict(os.environ, PARITY_CHROME_PATH=CHROME), check=True, timeout=180)
    r = json.load(open(p + '.chrome.json'))
    print(f"{os.path.basename(p)[:-5]:20s}", [(k, [round(x, 1) for x in r[k][:4]]) for k in ('w', 'c', 'after') if k in r])
