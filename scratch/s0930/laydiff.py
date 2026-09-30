"""Layout dump of a page under two parity-capture binaries; print the first boxes that differ.
usage: python3 laydiff.py <binA> <binB> <page.html>"""
import json, subprocess, sys, os
A, B, P = sys.argv[1:4]
out = []
for i, b in enumerate((A, B)):
    j = f'/tmp/laydiff-{i}.json'
    subprocess.run([b, '--html-file', P, '--dump-layout', j, '--dump-frame', f'/tmp/laydiff-{i}.ppm'],
                   capture_output=True, timeout=300)
    out.append(json.load(open(j)))


def flat(nd, path, acc):
    bb = nd.get('border_box') or {}
    acc.append((path + '/' + (nd.get('selector') or nd.get('tag') or nd.get('box_type', '?')),
                round(bb.get('x', 0)), round(bb.get('y', 0)), round(bb.get('width', 0)), round(bb.get('height', 0))))
    for k, c in enumerate(nd.get('children', [])):
        flat(c, path + f'/{k}', acc)
    return acc


fa, fb = flat(out[0]['root'], '', []), flat(out[1]['root'], '', [])
print('boxes', len(fa), len(fb))
# boxes present in B only (by selector name, ignoring index path)
na = {}
for p, *r in fa:
    na.setdefault(p.split('/')[-1], []).append(r)
nb = {}
for p, *r in fb:
    nb.setdefault(p.split('/')[-1], []).append(r)
for k in nb:
    if len(nb[k]) != len(na.get(k, [])):
        print('count', k, len(na.get(k, [])), '->', len(nb[k]), nb[k][:3])
shown = 0
for k in na:
    if k in nb and len(na[k]) == len(nb[k]) and na[k] != nb[k]:
        print('moved', k, na[k][:2], '->', nb[k][:2])
        shown += 1
        if shown > 25:
            break
