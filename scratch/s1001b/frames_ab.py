"""RustKit-only A/B on live URLs, no Chrome: capture each URL A,B,A,B and print, per URL, the frame
difference within an arm (live variance) and across arms, plus the text font families painted.
usage: frames_ab.py <binA> <binB> <name=url> [...]"""
import itertools, json, os, subprocess, sys
from PIL import Image, ImageChops
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite'
OUT = f'{HUB}/scratch/s1001b/frames'
os.makedirs(OUT, exist_ok=True)
a, b = sys.argv[1:3]


def shot(binp, url, stem):
    r = subprocess.run([binp, '--url', url, '--width', '1280', '--height', '800', '--dump-frame', stem + '.ppm'],
                       capture_output=True, text=True, timeout=120)
    return Image.open(stem + '.ppm').convert('RGB') if r.returncode == 0 and os.path.exists(stem + '.ppm') else None


def pct(x, y):
    if x is None or y is None:
        return float('nan')
    d = ImageChops.difference(x, y).convert('L')
    return 100.0 * sum(1 for p in d.getdata() if p > 8) / (x.size[0] * x.size[1])


for spec in sys.argv[3:]:
    name, url = spec.split('=', 1)
    f = {}
    for k, binp in (('A1', a), ('B1', b), ('A2', a), ('B2', b)):
        f[k] = shot(binp, url, f'{OUT}/{name}-{k}')
        if f[k]:
            f[k].save(f'{OUT}/{name}-{k}.png')
    print(f"{name:12} within develop {pct(f['A1'], f['A2']):6.2f}%  within fix {pct(f['B1'], f['B2']):6.2f}%  "
          f"across " + ' '.join(f"{pct(f[x], f[y]):6.2f}%" for x, y in itertools.product(('A1', 'A2'), ('B1', 'B2'))),
          flush=True)
