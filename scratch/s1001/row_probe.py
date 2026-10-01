"""What makes Chrome's list-box option row 16 vs 17 px: body font, line-height, the select's own font."""
import json, subprocess
sel = ('<select id="s" size="3"><option id="o">Item 1</option><option>Item 2</option></select>'
       '<select id="m"><option>Item 1</option></select><input id="i">')
V = {'plain': '', 'arial16': 'body{font:16px Arial}', 'lh15': 'body{line-height:1.5}',
     'sysui': 'body{font-family:system-ui}', 'sans14': 'body{font:14px sans-serif}',
     'selfont': 'select{font:16px Georgia}', 'helv': 'body{font:16px Helvetica}'}
for k, css in V.items():
    p = f'scratch/s1001/probe-{k}.html'
    open(p, 'w').write(f'<!DOCTYPE html><style>{css}</style><body>{sel}</body>')
    subprocess.run(['python3', 'scratch/s1001/chrome_shot.py', p, p[:-5] + '-chrome'], check=True, capture_output=True)
    d = json.load(open(p[:-5] + '-chrome.json'))
    print(k, {i: [round(x, 2) for x in v['rect']] for i, v in d.items()})
