"""Capture a URL with one binary and list the rounded / solid commands whose rect starts inside a region.
usage: dl_one.py <binary-name> <stem> <url> <x0> <y0> <x1> <y1>"""
import json, subprocess, sys
BIN = '/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/bin'
OUT = '/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/s1001c'
binary, stem, url = sys.argv[1:4]
x0, y0, x1, y1 = map(float, sys.argv[4:8])
stem = f'{OUT}/{stem}'
r = subprocess.run([f'{BIN}/{binary}', '--url', url, '--width', '1280', '--height', '800',
                    '--dump-frame', stem + '.ppm', '--dump-display-list', stem + '.dl.json'],
                   capture_output=True, text=True, timeout=280)
print('rc', r.returncode, (r.stdout + r.stderr)[-400:])
j = json.load(open(stem + '.dl.json'))
cmds = j.get('commands') if isinstance(j, dict) else j
print(len(cmds), 'commands')
for c in cmds:
    rect = c.get('rect') or {}
    if c.get('op') in ('text', 'glyphs', 'image'):
        continue
    if x0 <= rect.get('x', -1) <= x1 and y0 <= rect.get('y', -1) <= y1 and rect.get('width', 0) > 100:
        print('  ', c.get('op'), {k: round(v, 2) for k, v in rect.items()}, c.get('radius') or c.get('border_radius') or '')
