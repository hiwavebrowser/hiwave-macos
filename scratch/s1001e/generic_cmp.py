"""Text width per family: Chrome 148 (fx/generic-chrome.json) against RustKit's display list for
generic.html (layout's advances, and the face the run names).
usage: generic_cmp.py <rustkit-binary>"""
import json, subprocess, sys
D = '/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/s1001e'
subprocess.run([sys.argv[1], '--html-file', f'{D}/generic.html', '--width', '800', '--height', '600',
                '--dump-frame', f'{D}/fx/generic-rk.ppm', '--dump-display-list', f'{D}/fx/generic-rk.dl.json'],
               capture_output=True, text=True, timeout=120)
chrome = json.load(open(f'{D}/fx/generic-chrome.json'))
j = json.load(open(f'{D}/fx/generic-rk.dl.json'))
cmds = j.get('commands') if isinstance(j, dict) else j
texts = [c for c in cmds if c.get('op') == 'text']
ids = ['unstyled', 'sans', 'serif', 'mono', 'nsans', 'nserif', 'nmono', 'none', 'helv', 'times', 'tnr', 'menlo',
       'sys', 'cursive', 'fantasy']
print('sample chrome entry:', chrome[ids[0]])
print(f'{"id":9} {"chrome w":>9} {"rustkit w":>9} {"delta":>7}  rustkit face')
for i, t in zip(ids, texts):
    c = chrome.get(i) or {}
    cw = c['rect'][2]
    ch = c['rect'][3]
    rw = sum(t.get('advances') or [])
    face = (t.get('run') or {}).get('face')
    print(f'{i:9} {cw:9.2f} {rw:9.2f} {rw - cw:7.2f}  {face}   (chrome line box {ch})')
