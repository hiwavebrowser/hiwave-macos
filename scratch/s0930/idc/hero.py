"""Where do linkedin's layered-variant shell boxes land? usage: python3 hero.py <bin>"""
import json, os, subprocess, sys
D = '/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/li0930'
out = f'{D}/lay-{os.path.basename(sys.argv[1])}.json'
subprocess.run([sys.argv[1], '--html-file', f'{D}/li-new.html', '--dump-layout', out], capture_output=True, timeout=240)
L = json.load(open(out))


def walk(nd, depth, path):
    sel = nd.get('selector') or ''
    tag = nd.get('tag') or ''
    if tag in ('main', 'h1', 'footer', 'header') or any(c in sel for c in ('auymhl', 'auymh2', 'auymgm')):
        b = nd['border_box']
        print('  ' * min(depth, 12), tag, sel[-70:], [round(b[k]) for k in ('x', 'y', 'width', 'height')],
              nd.get('display', ''), nd.get('box_type', ''))
    for c in nd.get('children', []):
        walk(c, depth + 1, path)


walk(L.get('root', L), 0, [])
