"""Apply rustfmt's suggestions ONLY to hunks inside lines this branch changed vs origin/develop
(the files carry ~2000 older fmt diffs that are not this PR's). usage: fmt_hunks.py <worktree>"""
import re, subprocess, sys
wt = sys.argv[1]
out = subprocess.run(['cargo', 'fmt', '--all', '--', '--check'], cwd=wt, capture_output=True, text=True).stdout
d = subprocess.run(['git', 'diff', '-U0', 'origin/develop'], cwd=wt, capture_output=True, text=True).stdout
changed, cur = {}, None
for l in d.splitlines():
    if l.startswith('+++ b/'):
        cur = l[6:]
    m = re.match(r'@@ -\S+ \+(\d+)(?:,(\d+))? @@', l)
    if m and cur:
        s, n = int(m.group(1)), int(m.group(2) or 1)
        changed.setdefault(cur, []).append((s, s + n))
edits = {}
for h in re.split(r'(?=^Diff in )', out, flags=re.M):
    m = re.match(r'Diff in (\S+?):(\d+):\n', h)
    if not m:
        continue
    f, line = m.group(1), int(m.group(2))
    rel = f[f.index('crates/'):]
    body = h[m.end():].split('\n')
    while body and body[-1] == '':
        body.pop()
    old = [b[1:] for b in body if b[:1] in (' ', '-')]
    new = [b[1:] for b in body if b[:1] in (' ', '+')]
    touched = [line + i for i, b in enumerate([b for b in body if b[:1] in (' ', '-')]) if b[:1] == '-']
    if not touched or not all(any(s <= t < e for s, e in changed.get(rel, [])) for t in touched):
        continue
    edits.setdefault(f, []).append(('\n'.join(old), '\n'.join(new)))
for f, es in edits.items():
    s = open(f).read()
    n = 0
    for old, new in es:
        if s.count(old) == 1:
            s = s.replace(old, new); n += 1
        else:
            print('skip (not unique):', f, old[:60].replace('\n', ' '))
    open(f, 'w').write(s)
    print(f[f.index('crates/'):], 'applied', n, 'of', len(es))
