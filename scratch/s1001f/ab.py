"""S0 real-site A/B, RustKit only (no Chrome): capture each URL A,B,A,B, print the frame difference
within each arm (the site's own variance) and across arms, and from the fix arm's display list how
many text commands carry a shaped run and which faces they name.
usage: s0_ab.py <binA> <binB> <name=url> [...]"""
import collections, itertools, json, os, subprocess, sys
from PIL import Image, ImageChops
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite'
OUT = f'{HUB}/scratch/s1001f/frames'
os.makedirs(OUT, exist_ok=True)
a, b = sys.argv[1:3]


def shot(binp, url, stem, dl=False):
    cmd = [binp, '--url', url, '--width', '1280', '--height', '800', '--dump-frame', stem + '.ppm']
    if dl:
        cmd += ['--dump-display-list', stem + '.dl.json']
    for f in (stem + '.ppm', stem + '.dl.json'):
        if os.path.exists(f):
            os.remove(f)
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=150)
    except subprocess.TimeoutExpired:
        return None
    return Image.open(stem + '.ppm').convert('RGB') if r.returncode == 0 and os.path.exists(stem + '.ppm') else None


def pct(x, y):
    if x is None or y is None or x.size != y.size:
        return float('nan')
    d = ImageChops.difference(x, y).convert('L')
    return 100.0 * sum(1 for p in d.getdata() if p > 8) / (x.size[0] * x.size[1])


def runs(stem):
    try:
        j = json.load(open(stem + '.dl.json'))
    except Exception:
        return 'no display list'
    cmds = j.get('commands') if isinstance(j, dict) else j
    texts = [c for c in cmds if c.get('op') == 'text']
    with_run = [c for c in texts if c.get('run')]
    faces = collections.Counter(c['run']['face'] for c in with_run)
    top = ', '.join(f'{k} {v}' for k, v in faces.most_common(3))
    return f'{len(with_run)}/{len(texts)} text commands carry a run ({top})'


for spec in sys.argv[3:]:
    name, url = spec.split('=', 1)
    f = {}
    for k, binp in (('A1', a), ('B1', b), ('A2', a), ('B2', b)):
        f[k] = shot(binp, url, f'{OUT}/{name}-{k}', dl=(k == 'B1'))
        if f[k]:
            f[k].save(f'{OUT}/{name}-{k}.png')
    print(f"{name:12} within develop {pct(f['A1'], f['A2']):6.2f}%  within fix {pct(f['B1'], f['B2']):6.2f}%  "
          f"across " + ' '.join(f"{pct(f[x], f[y]):6.2f}%" for x, y in itertools.product(('A1', 'A2'), ('B1', 'B2')))
          + '  | ' + runs(f'{OUT}/{name}-B1'), flush=True)
