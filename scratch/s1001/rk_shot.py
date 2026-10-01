"""Capture a local page with a parity-capture binary at 1280x800; write png, a 2x crop, and the display list.
usage: rk_shot.py <binary> <file.html> <out-stem> [crop_w crop_h]"""
import os, subprocess, sys
from PIL import Image
b, html, stem = sys.argv[1], os.path.abspath(sys.argv[2]), sys.argv[3]
w, h = (int(sys.argv[4]), int(sys.argv[5])) if len(sys.argv) > 5 else (640, 260)
r = subprocess.run([b, '--html-file', html, '--width', '1280', '--height', '800', '--dump-frame', stem + '.ppm',
                    '--dump-display-list', stem + '.dl.json'], capture_output=True, text=True, timeout=180)
if r.returncode:
    print(r.stdout[-1500:], r.stderr[-1500:])
    sys.exit(r.returncode)
im = Image.open(stem + '.ppm').convert('RGB')
im.save(stem + '.png')
im.crop((0, 0, w, h)).resize((w * 2, h * 2), Image.NEAREST).save(stem + '-zoom.png')
print('->', stem + '.png', im.size)
