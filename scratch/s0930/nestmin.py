"""Render nest-min.html with each binary and print a few pixels.
usage: python3 nestmin.py <bin>..."""
import os, subprocess, sys
from PIL import Image
D = os.path.dirname(os.path.abspath(__file__))
page = sys.argv[1] if sys.argv[1].endswith('.html') else f'{D}/nest-min.html'
bins = [a for a in sys.argv[1:] if not a.endswith('.html')]
for b in bins:
    out = f'{D}/nestmin-{os.path.basename(b)}.ppm'
    r = subprocess.run([b, '--html-file', page, '--dump-frame', out], capture_output=True, text=True, timeout=120)
    im = Image.open(out).convert('RGB')
    pts = {p: im.getpixel(p) for p in [(10, 10), (90, 40), (10, 45), (10, 60), (10, 85)]}
    print(os.path.basename(b), r.returncode, pts)
