"""usage: red_boxes.py <bin>... : capture ratio.html at 400x1000 per bin, print the red bbox in each 160px row."""
import os, subprocess, sys
from PIL import Image
here = os.path.dirname(os.path.abspath(__file__))
for binary in sys.argv[1:]:
    out = os.path.join(here, 'ratio-' + os.path.basename(binary) + '.ppm')
    p = subprocess.run([binary, '--html-file', os.path.join(here, 'ratio.html'), '--width', '400', '--height', '1000',
                        '--dump-frame', out], capture_output=True, text=True)
    if p.returncode:
        print(binary, 'exit', p.returncode, p.stderr[-400:]); continue
    im = Image.open(out).convert('RGB'); px = im.load()
    rows = []
    for r, name in enumerate('abcdef'):
        pts = [(x, y) for y in range(r * 160, min(r * 160 + 160, im.height)) for x in range(im.width)
               if px[x, y][0] > 200 and px[x, y][1] < 60 and px[x, y][2] < 60]
        if pts:
            xs = [q[0] for q in pts]; ys = [q[1] for q in pts]
            rows.append(f'{name}: {min(xs)} {min(ys)} {max(xs)-min(xs)+1} {max(ys)-min(ys)+1}')
        else:
            rows.append(f'{name}: none')
    print(os.path.basename(binary), ' | '.join(rows))
