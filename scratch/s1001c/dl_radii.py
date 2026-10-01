"""Capture a URL with two parity-capture binaries and list the rounded display-list commands inside a
region, per arm. usage: dl_radii.py <name> <url> <x0> <y0> <x1> <y1>"""
import json, subprocess, sys
BIN = '/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/bin'
OUT = '/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/s1001c'
name, url = sys.argv[1:3]
x0, y0, x1, y1 = map(float, sys.argv[3:7])
for arm, binary in (('dev', 'pc-dev-f16ad4e'), ('fix', 'pc-ellipse-740efcb')):
    stem = f'{OUT}/{name}-{arm}'
    r = subprocess.run([f'{BIN}/{binary}', '--url', url, '--width', '1280', '--height', '800',
                        '--dump-frame', stem + '.ppm', '--dump-display-list', stem + '.dl.json'],
                       capture_output=True, text=True, timeout=150)
    if r.returncode:
        print(arm, 'capture failed', r.stderr[-300:])
        continue
    j = json.load(open(stem + '.dl.json'))
    cmds = j.get('commands') if isinstance(j, dict) else j
    print(arm, len(cmds), 'commands')
    for c in cmds:
        rect = c.get('rect') or {}
        rad = c.get('radius') or c.get('border_radius')
        if rad is None and c.get('op') != 'unknown':
            continue
        if x0 <= rect.get('x', -1) <= x1 and y0 <= rect.get('y', -1) <= y1:
            print('  ', c.get('op'), {k: round(v, 2) for k, v in rect.items()}, rad)
