"""Chrome's button rects (cached <page>.chrome.json) against RustKit's form-control boxes in document order.
Ids that are not leaf controls in RustKit (a button with element children) are skipped by name.
usage: btn_cmp.py <binary> <page.html> [skip-id,...]"""
import json, os, subprocess, sys
binary, html = sys.argv[1], os.path.abspath(sys.argv[2])
skip = set(sys.argv[3].split(',')) if len(sys.argv) > 3 else set()
stem = html[:-5]
out = stem + '.' + os.path.basename(binary)
r = subprocess.run([binary, '--html-file', html, '--width', '1280', '--height', '800',
                    '--dump-frame', out + '.ppm', '--dump-layout', out + '.layout.json'],
                   capture_output=True, text=True, timeout=180)
if r.returncode:
    print('rustkit rc', r.returncode, r.stderr[-800:])
c = json.load(open(stem + '.chrome.json'))


def walk(n):
    yield n
    for ch in n.get('children', []):
        yield from walk(ch)


rk = [n['border_box'] for n in walk(json.load(open(out + '.layout.json'))['root']) if n.get('type') == 'form_control']
ids = [k for k in c if k not in skip]
bad = 0
print(f'{"id":9} {"chrome x,y,w,h":32} {"rustkit":32} off')
for k, b in zip(ids, rk):
    cr = [round(v, 2) for v in c[k]['rect']]
    rr = [round(b[a], 2) for a in ('x', 'y', 'width', 'height')]
    d = [round(x - y, 2) for x, y in zip(rr, cr)]
    off = any(abs(v) > 0.5 for v in d)
    bad += off
    print(f'{k:9} {str(cr):32} {str(rr):32} {d if off else "ok"}')
print(f'{bad} of {len(ids)} boxes off by more than 0.5px ({len(rk)} rustkit controls)')
