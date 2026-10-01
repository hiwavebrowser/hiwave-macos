"""Screenshot a local page with pinned Chrome 148 (tools/parity_oracle/shot_local.mjs) and write a 2x crop.
usage: chrome_shot.py <file.html> <out-stem> [crop_w crop_h]"""
import os, subprocess, sys
from PIL import Image
HERE = os.path.dirname(os.path.abspath(__file__))
CHROME = os.path.expanduser('~/Repos/hiwave/hiwave-macos/.browsers/chrome/mac_arm-148.0.7778.216/chrome-mac-arm64/'
                            'Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing')
html, stem = sys.argv[1], sys.argv[2]
w, h = (int(sys.argv[3]), int(sys.argv[4])) if len(sys.argv) > 4 else (640, 260)
subprocess.run(['node', os.path.join(HERE, '../../tools/parity_oracle/shot_local.mjs'), html, stem + '.png',
                stem + '.json', 'display'], env=dict(os.environ, PARITY_CHROME_PATH=CHROME), check=True, timeout=180)
Image.open(stem + '.png').convert('RGB').crop((0, 0, w, h)).resize((w * 2, h * 2), Image.NEAREST).save(stem + '-zoom.png')
print('->', stem + '.png')
