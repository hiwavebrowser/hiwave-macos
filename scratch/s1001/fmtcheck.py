"""rustfmt --check a worktree; print only the diff hunks whose file+line fall in lines changed vs origin/develop.
usage: fmtcheck.py <worktree>"""
import re, subprocess, sys
wt = sys.argv[1]
r = subprocess.run(['cargo', 'fmt', '--all', '--', '--check'], cwd=wt, capture_output=True, text=True)
out = r.stdout + r.stderr
d = subprocess.run(['git', 'diff', '-U0', 'origin/develop'], cwd=wt, capture_output=True, text=True).stdout
changed, cur = {}, None
for l in d.splitlines():
    if l.startswith('+++ b/'):
        cur = l[6:]
    m = re.match(r'@@ -\S+ \+(\d+)(?:,(\d+))? @@', l)
    if m and cur:
        s, n = int(m.group(1)), int(m.group(2) or 1)
        changed.setdefault(cur, []).append((s, s + n))
hits = 0
for m in re.finditer(r'Diff in (\S+?):(\d+):?', out):
    f, line = m.group(1), int(m.group(2))
    rel = f[f.index('crates/'):] if 'crates/' in f else f
    if any(s - 3 <= line <= e + 3 for s, e in changed.get(rel, [])):
        hits += 1
        print('fmt diff in changed region:', rel, line)
print('rc', r.returncode, 'total fmt diffs', out.count('Diff in'), 'in my lines', hits)
