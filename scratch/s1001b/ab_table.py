"""Table for the chunked real-site A/B (ab_all.py). For every site with both arms:
- the board's own checks from this run (fresh Chrome per arm), and
- because the fresh oracle timed out on many captures, each arm's RustKit frame against ONE stored
  oracle frame (chrome-a.png of the last quiet board), and A's frame against B's.
usage: ab_table.py <tag> [stored-run]"""
import glob, json, os, subprocess, sys
from PIL import Image
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite'
RUNS = f'{HUB}/trench/realsite/runs'
tag = sys.argv[1]
stored = sys.argv[2] if len(sys.argv) > 2 else '20261001T0226Z-quiet-devaf4b95d'


def diff(a_png, b):
    r = subprocess.run(['node', f'{HUB}/tools/parity_oracle/realsite.mjs', 'diff', a_png, b], cwd=HUB,
                       capture_output=True, text=True, timeout=180)
    try:
        j = json.loads(r.stdout.strip().splitlines()[-1])
        return j.get('diffPercent', j.get('diff_pct'))
    except Exception:
        return None


def arm(site, letter):
    hits = glob.glob(f'{RUNS}/{tag}-c*-{letter}/{site}.json')
    return (json.load(open(hits[0])), os.path.dirname(hits[0])) if hits else (None, None)


def fmt(v):
    return '   -  ' if v is None else f'{v:6.2f}'


def cell(d):
    lr, rd = d['looks_right'], d['readable']
    lrp, ratio = lr.get('diff'), rd.get('ratio')
    oracle_failed = lr.get('oracle_failed') or rd.get('oracle_failed')
    return (f"{d['points']}/3 loads {'PASS' if d['loads']['pass'] else 'fail'}, "
            + ('oracle failed' if oracle_failed else
               f"readable {ratio * 100:.1f}%, looks {lrp:.1f}%" if ratio is not None and lrp is not None else
               f"readable {ratio}, looks {lrp}"))


sites = sorted({os.path.basename(p)[:-5] for p in glob.glob(f'{RUNS}/{tag}-c*-A/*.json')} - {'summary'})
tot = {'A': 0, 'B': 0}
print('site | develop (this run) | fix (this run) | dev vs stored oracle % | fix vs stored oracle % | dev vs fix frame %')
for s in sites:
    (a, da), (b, db) = arm(s, 'A'), arm(s, 'B')
    if not a or not b:
        continue
    tot['A'] += a['points']; tot['B'] += b['points']
    fa, fb = f'{da}/{s}/rustkit.ppm', f'{db}/{s}/rustkit.ppm'
    oracle = f'{RUNS}/{stored}/{s}/chrome-a.png'
    va = vb = ab = None
    if os.path.exists(fa) and os.path.exists(fb):
        if os.path.exists(oracle):
            va, vb = diff(oracle, fa), diff(oracle, fb)
        png = f'{da}/{s}/rustkit.png'
        Image.open(fa).convert('RGB').save(png)
        ab = diff(png, fb)
    print(f'{s:11} | {cell(a)} | {cell(b)} | {fmt(va)} | {fmt(vb)} | {fmt(ab)}')
print('points on these sites: develop', tot['A'], 'fix', tot['B'])
