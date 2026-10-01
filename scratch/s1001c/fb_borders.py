"""facebook's login inputs: which border strips does a binary paint around the email box (x 692, y 201,
536x60)? Prints every solid_color / rounded command that starts on the box's edges.
usage: fb_borders.py <binary-name> <stem>"""
import json, subprocess, sys
BIN = '/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/bin'
OUT = '/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/s1001c'
binary, stem = sys.argv[1:3]
stem = f'{OUT}/{stem}'
r = subprocess.run([f'{BIN}/{binary}', '--url', 'https://www.facebook.com/', '--width', '1280', '--height', '800',
                    '--dump-frame', stem + '.ppm', '--dump-display-list', stem + '.dl.json'],
                   capture_output=True, text=True, timeout=280)
print('rc', r.returncode)
j = json.load(open(stem + '.dl.json'))
cmds = j.get('commands') if isinstance(j, dict) else j
for c in cmds:
    rect = c.get('rect') or {}
    if c.get('op') in ('text', 'glyphs', 'image'):
        continue
    x, y, w, h = (rect.get(k, -1) for k in ('x', 'y', 'width', 'height'))
    if 690 <= x <= 1230 and 200 <= y <= 262 and (w >= 500 or h >= 50):
        print('  ', c.get('op'), {k: round(v, 2) for k, v in rect.items()})
