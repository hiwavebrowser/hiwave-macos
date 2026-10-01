"""Walk a parity-capture --dump-layout JSON: print the top-level keys of a node, then the chain of
ancestors (and direct children) of the first node whose selector contains <needle>.
usage: python3 walk.py <layout.json> <needle> [child-depth=2]"""
import json, sys
L = json.load(open(sys.argv[1]))
needle = sys.argv[2]
cd = int(sys.argv[3]) if len(sys.argv) > 3 else 2
root = L.get('root', L)


def box(nd):
    for k in ('border_box', 'rect', 'content_box', 'bounds'):
        b = nd.get(k)
        if isinstance(b, dict):
            return [round(b.get(x, 0), 1) for x in ('x', 'y', 'width', 'height')]
    return None


def desc(nd):
    return f"{nd.get('tag') or nd.get('type')} {str(nd.get('selector')).split(' > ')[-1][:110]} {box(nd)} {nd.get('display', '')} {nd.get('box_type', '')}"


def find(nd, path):
    if needle in (nd.get('selector') or ''):
        return path + [nd]
    for c in nd.get('children', []):
        r = find(c, path + [nd])
        if r:
            return r


def dump(nd, depth, ind):
    print('  ' * ind + desc(nd) + (' text=' + repr(nd.get('text'))[:40] if nd.get('text') else ''))
    if depth:
        for c in nd.get('children', []):
            dump(c, depth - 1, ind + 1)


p = find(root, [])
if not p:
    print('not found; root keys', list(root.keys()))
    sys.exit(1)
print('node keys:', list(p[-1].keys()))
for i, nd in enumerate(p[:-1]):
    print(' ' * min(i, 30) + desc(nd))
print('--- target subtree')
dump(p[-1], cd, 0)
