"""@layer probes: write pages, capture each with a binary, print solid-colour rects.
usage: probe.py <binary>"""
import json, os, subprocess, sys, tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
B = os.path.abspath(sys.argv[1])
BOX = 'width:100px;height:50px'
PAGES = {
    'plain': f'<style>#x{{background:red;{BOX}}} #y{{background:blue;{BOX}}}</style>',
    'stmt': f'<style>@layer a, b; #x{{background:red;{BOX}}} #y{{background:blue;{BOX}}}</style>',
    'block': f'<style>@layer a {{ #x{{background:red;{BOX}}} }} #y{{background:blue;{BOX}}}</style>',
    'both': f'<style>@layer a, b; @layer a {{ #x{{background:red;{BOX}}} }} #y{{background:blue;{BOX}}}</style>',
    'anon': f'<style>@layer {{ #x{{background:red;{BOX}}} }} #y{{background:blue;{BOX}}}</style>',
    'nested': f'<style>@layer a {{ @media (min-width:10px) {{ #x{{background:red;{BOX}}} }} }} #y{{background:blue;{BOX}}}</style>',
    # layer order: unlayered beats layered; later layer beats earlier
    'order': f'<style>@layer a, b; @layer b {{ #x{{background:green;{BOX}}} }} @layer a {{ #x{{background:red}} }} #y{{background:blue;{BOX}}} @layer b {{ #y{{background:red}} }}</style>',
}
d = os.path.join(HERE, 'probe'); os.makedirs(d, exist_ok=True)
for name, style in PAGES.items():
    p = os.path.join(d, name + '.html')
    open(p, 'w').write(f'<!doctype html>{style}<div id=x></div><div id=y></div>')
    out = tempfile.mkdtemp(); dl = os.path.join(out, 'dl.json')
    r = subprocess.run([B, '--html-file', p, '--width', '400', '--height', '300', '--dump-frame', out + '/f.ppm',
                        '--dump-display-list', dl], capture_output=True, text=True, timeout=120)
    try:
        ops = json.load(open(dl))
    except Exception as e:
        print(name, 'rc', r.returncode, 'no dl', r.stderr[-300:]); continue
    ops = ops.get('commands', ops) if isinstance(ops, dict) else ops
    rects = [json.dumps(o)[:140] for o in ops if 'Rect' in json.dumps(o)[:40]]
    print(f'{name:7}', rects[:4] if rects else [json.dumps(o)[:120] for o in ops[:3]])
