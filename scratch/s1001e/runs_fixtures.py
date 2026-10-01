"""Run coverage on local fixtures: capture each html file with <bin>, and with <dev-bin> when given,
and report (a) how many text commands carry a shaped run and the faces named, (b) whether the two
binaries' frames are byte-identical.
usage: runs_fixtures.py <fix-bin> <dev-bin|-> <html> [...]"""
import collections, json, os, subprocess, sys
OUT = '/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/s1001e/fx'
os.makedirs(OUT, exist_ok=True)
fix, dev = sys.argv[1:3]
tot_run = tot_text = same = n = 0
faces = collections.Counter()
norun = collections.Counter()
paths = []
for arg in sys.argv[3:]:
    if arg.startswith('@'):  # @<repo>: every campaign case of that checkout
        out = subprocess.run([sys.executable, os.path.dirname(os.path.abspath(__file__)) + '/list_cases.py', arg[1:]],
                             capture_output=True, text=True).stdout
        paths += out.split()
    else:
        paths.append(arg)
for path in paths:
    name = os.path.splitext(os.path.basename(path))[0]
    if name == 'index':
        name = os.path.basename(os.path.dirname(path))
    stem = f'{OUT}/{name}'
    r = subprocess.run([fix, '--html-file', path, '--width', '1280', '--height', '800',
                        '--dump-frame', stem + '-fix.ppm', '--dump-display-list', stem + '.dl.json'],
                       capture_output=True, text=True, timeout=200)
    if r.returncode != 0:
        print(name, 'fix capture failed', (r.stdout + r.stderr)[-200:])
        continue
    j = json.load(open(stem + '.dl.json'))
    cmds = j.get('commands') if isinstance(j, dict) else j
    texts = [c for c in cmds if c.get('op') == 'text']
    with_run = [c for c in texts if c.get('run')]
    for c in with_run:
        faces[c['run']['face']] += 1
    for c in texts:
        if not c.get('run'):
            norun[c.get('text', '')[:24]] += 1
    tot_run += len(with_run)
    tot_text += len(texts)
    line = f'{name:28} {len(with_run):4}/{len(texts):<4} runs'
    if dev != '-':
        r = subprocess.run([dev, '--html-file', path, '--width', '1280', '--height', '800',
                            '--dump-frame', stem + '-dev.ppm'], capture_output=True, text=True, timeout=200)
        if r.returncode == 0:
            ident = open(stem + '-dev.ppm', 'rb').read() == open(stem + '-fix.ppm', 'rb').read()
            n += 1
            same += ident
            line += '  frame ' + ('identical' if ident else 'DIFFERS')
    print(line, flush=True)
print(f'TOTAL {tot_run}/{tot_text} text commands carry a run; frames identical {same}/{n}')
print('faces:', ', '.join(f'{k} {v}' for k, v in faces.most_common(12)))
print('without a run:', ', '.join(f'{k!r} {v}' for k, v in norun.most_common(12)))
