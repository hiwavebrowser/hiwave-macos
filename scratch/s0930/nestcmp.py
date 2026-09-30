"""Compare two parity-capture binaries on linkedin's saved layered variant: frame identity and
the grid-stack (`auya49`) boxes with their children.
usage: python3 nestcmp.py <binA> <binB>"""
import filecmp, json, os, subprocess, sys
D = '/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/li0930'


def layout(b):
    out = f'{D}/lay-{os.path.basename(b)}.json'
    if not os.path.exists(out):
        subprocess.run([b, '--html-file', f'{D}/li-new.html', '--dump-layout', out], capture_output=True, timeout=300)
    return json.load(open(out))


def stacks(nd, acc):
    if 'auya49' in (nd.get('selector') or ''):
        kids = [(c.get('tag'), [round(c['border_box'][k]) for k in ('x', 'y', 'width', 'height')],
                 c.get('display', '')) for c in nd.get('children', [])[:4]]
        b = nd['border_box']
        acc.append(([round(b[k]) for k in ('x', 'y', 'width', 'height')], nd.get('display', ''), kids))
    for c in nd.get('children', []):
        stacks(c, acc)
    return acc


a, b = sys.argv[1], sys.argv[2]
fa = f'{D}/off-{os.path.basename(a)}.ppm'
fb = f'{D}/off-{os.path.basename(b)}.ppm'
if os.path.exists(fa) and os.path.exists(fb):
    print('frames identical:', filecmp.cmp(fa, fb, shallow=False))
for bin_ in (a, b):
    L = layout(bin_)
    print('==', os.path.basename(bin_))
    for s in stacks(L.get('root', L), [])[:6]:
        print('  ', s)
