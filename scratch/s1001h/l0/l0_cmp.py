"""L0 fixture: Chrome's rect by id against a RustKit layout dump, with each item's children.
usage: l0_cmp.py <label>"""
import json, sys
D = '/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/s1001h/l0'
c = json.load(open(f'{D}/chrome.json'))
j = json.load(open(f'{D}/{sys.argv[1]}.layout.json'))
root = j['root']
print('node keys:', [k for k in root if k != 'children'])


def walk(n, depth, path):
    yield n, depth, path
    for i, ch in enumerate(n.get('children', [])):
        yield from walk(ch, depth + 1, path + [i])


def rect(n):
    b = n['border_box']
    return [round(b[k], 2) for k in ('x', 'y', 'width', 'height')]


def ident(n):
    for k in ('id', 'element_id', 'selector', 'dom_id'):
        if n.get(k):
            return str(n[k])
    return ''


# The dump carries no ids: the grid is the first 620-wide box, its children are the items in order.
ORDER = ['wrap', 'many', 'short', 'wrap2', 'span', 'block', 'ratio', 'pct']
grid = next(n for n, _, _ in walk(root, 0, []) if rect(n)[2] == 620.0)
kids = [k for k in grid['children']]
assert len(kids) == len(ORDER), (len(kids), [rect(k) for k in kids])
named = [('grid', grid)] + list(zip(ORDER, kids))
for key, n in named:
    if key in c:
        cr = [round(x, 2) for x in c[key]['rect']]
        rr = rect(n)
        bad = [k for k, a, b in zip('xywh', cr, rr) if abs(a - b) > 0.5]
        print(f'{key:8} chrome {cr}  rustkit {rr}  {"OFF " + "".join(bad) if bad else "ok"}')
        for ch in n.get('children', []):
            if 'border_box' not in ch:
                continue
            print(f'           child {ch.get("box_type", ch.get("type", ""))!s:12.12} {rect(ch)} {ident(ch)} '
                  f'{str(ch.get("text", ""))[:30]!r}')
