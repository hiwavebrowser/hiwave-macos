"""Capture an HTML file with a parity-capture binary; print the display-list text ops and solid rects.
usage: cap.py <binary> <html> [--help]"""
import json, os, subprocess, sys, tempfile
b = sys.argv[1]
if sys.argv[2] == '--help':
    r = subprocess.run([b, '--help'], capture_output=True, text=True); print(r.stdout + r.stderr); sys.exit()
html = os.path.abspath(sys.argv[2])
out = tempfile.mkdtemp()
dl = os.path.join(out, 'dl.json')
r = subprocess.run([b, '--html-file', html, '--width', '800', '--height', '600', '--out', os.path.join(out, 'f.ppm'),
                    '--display-list', dl], capture_output=True, text=True, timeout=120)
if r.returncode:
    print(r.stdout[-2000:], r.stderr[-2000:])
try:
    ops = json.load(open(dl))
except Exception as e:
    print('no dl', e); sys.exit(1)
ops = ops.get('commands', ops) if isinstance(ops, dict) else ops
for o in ops[:80]:
    s = json.dumps(o)
    if len(s) > 200:
        s = s[:200]
    print(s)
