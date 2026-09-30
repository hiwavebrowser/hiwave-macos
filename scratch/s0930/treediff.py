"""Structural diff of /tmp/laydiff-{0,1}.json (from laydiff.py)."""
import difflib, json


def flat(nd, acc, d=0):
    acc.append((d, nd.get('selector'), nd.get('tag'), nd.get('box_type'), nd.get('border_box')))
    for c in nd.get('children', []):
        flat(c, acc, d + 1)
    return acc


ja, jb = json.load(open('/tmp/laydiff-0.json')), json.load(open('/tmp/laydiff-1.json'))
a, b = flat(ja['root'], []), flat(jb['root'], [])
sa = [f'{d} {s} {t} {bt}' for d, s, t, bt, _ in a]
sb = [f'{d} {s} {t} {bt}' for d, s, t, bt, _ in b]
idx = 0
for op in difflib.ndiff(sa, sb):
    if op[0] == '+':
        i = sb.index(op[2:])
        print(op, b[i][4], '| prev:', sb[i - 1], '| next:', sb[i + 1] if i + 1 < len(sb) else None)
    elif op[0] == '-':
        print(op)
print('root keys', list(jb['root'].keys()))
