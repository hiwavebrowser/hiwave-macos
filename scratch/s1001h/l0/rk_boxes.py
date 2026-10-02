"""RustKit boxes of a local page as an indented tree (border boxes), no Chrome.
usage: rk_boxes.py <binary> <page.html> <label> [max-depth]"""
import json, os, subprocess, sys
binary, html, label = sys.argv[1], os.path.abspath(sys.argv[2]), sys.argv[3]
maxd = int(sys.argv[4]) if len(sys.argv) > 4 else 6
stem = html[:-5]
lj = f'{stem}.{label}.layout.json'
r = subprocess.run([binary, '--html-file', html, '--width', '1280', '--height', '800',
                    '--dump-frame', f'{stem}.{label}.ppm', '--dump-layout', lj],
                   capture_output=True, text=True, timeout=180)
if r.returncode:
    print('rustkit rc', r.returncode, r.stderr[-800:])


def walk(n, d):
    if 'border_box' in n and d <= maxd:
        b = n['border_box']
        print('  ' * d + f"{n.get('type', '')} {[round(b[k], 2) for k in ('x', 'y', 'width', 'height')]}")
    for ch in n.get('children', []):
        walk(ch, d + 1)


walk(json.load(open(lj))['root'], 0)
