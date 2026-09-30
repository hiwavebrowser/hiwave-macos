"""Does each binary apply `#a.x{display:none}`? And apple's globalnav rules on the saved page?
usage: python3 probe.py <bin>..."""
import json, os, subprocess, sys
HERE = os.path.dirname(os.path.abspath(__file__))
t = os.path.join(HERE, 't.html')
open(t, 'w').write('<!doctype html><style>#a.x{display:none}</style><body><p id=a class=x>one</p><p>two</p></body>')
for b in sys.argv[1:]:
    name = os.path.basename(b)
    for page in [t] + [os.path.join(HERE, '..', '..', p) for p in ('apple-now.html',)]:
        out = os.path.join(HERE, f'{name}-{os.path.basename(page)}.json')
        subprocess.run([b, '--html-file', page, '--dump-layout', out], capture_output=True, timeout=180)
        L = json.load(open(out))
        n = 0; boxes = []
        def walk(nd):
            global n
            n += 1
            if 'globalnav' in (nd.get('selector') or ''):
                bb = nd['border_box']; boxes.append((nd['selector'][-40:], round(bb['y']), round(bb['height']), round(bb['width'])))
            for c in nd.get('children', []):
                walk(c)
        walk(L.get('root', L))
        print(name, os.path.basename(page), 'nodes', n, boxes[:4])
