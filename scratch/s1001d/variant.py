"""Does a live site's variant follow the binary or the request? Capture <url> in the order B A A B B A and say,
for each capture, which of the two reference frames (frames/<name>-A1.png, -B1.png) it is closer to.
usage: variant.py <name> <url> <binA> <binB>"""
import os, subprocess, sys
from PIL import Image, ImageChops
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite'
name, url, a, b = sys.argv[1:5]
ref = {k: Image.open(f'{HUB}/scratch/s1001b/frames/{name}-{k}1.png').convert('RGB') for k in 'AB'}


def pct(x, y):
    d = ImageChops.difference(x, y).convert('L')
    return 100.0 * sum(1 for p in d.tobytes() if p > 8) / (x.size[0] * x.size[1])


for i, k in enumerate('BAABBA'):
    stem = f'{HUB}/scratch/s1001d/{name}-var-{i}{k}'
    r = subprocess.run([a if k == 'A' else b, '--url', url, '--width', '1280', '--height', '800',
                        '--dump-frame', stem + '.ppm', '--dump-display-list', stem + '.dl.json'],
                       capture_output=True, text=True, timeout=120)
    if r.returncode or not os.path.exists(stem + '.ppm'):
        print(i, k, 'capture failed')
        continue
    im = Image.open(stem + '.ppm').convert('RGB')
    im.save(stem + '.png')
    print(f'{i} binary {k}: vs develop ref {pct(im, ref["A"]):5.2f}%  vs fix ref {pct(im, ref["B"]):5.2f}%', flush=True)
