"""Restore a file's original (mixed) line endings after an edit normalised them to LF.
Unchanged lines get their HEAD bytes back; changed/inserted lines take the ending of the
nearest preceding original line. usage: keep_eol.py <worktree> <path>"""
import difflib, subprocess, sys
wt, path = sys.argv[1:3]
orig = subprocess.run(['git', 'show', f'HEAD:{path}'], cwd=wt, capture_output=True).stdout.splitlines(keepends=True)
new = open(f'{wt}/{path}', 'rb').read().splitlines(keepends=True)
strip = lambda ls: [l.rstrip(b'\r\n') for l in ls]
eol = lambda l: l[len(l.rstrip(b'\r\n')):]
out, last = [], b'\n'
sm = difflib.SequenceMatcher(None, strip(orig), strip(new), autojunk=False)
for tag, i1, i2, j1, j2 in sm.get_opcodes():
    if tag == 'equal':
        out += orig[i1:i2]
        last = eol(orig[i2 - 1]) or last
    elif tag in ('replace', 'insert'):
        near = eol(orig[i1]) if i1 < len(orig) and tag == 'replace' else last
        out += [l.rstrip(b'\r\n') + (near or last) for l in new[j1:j2]]
open(f'{wt}/{path}', 'wb').write(b''.join(out))
print(subprocess.run(['git', 'diff', '--stat', '--', path], cwd=wt, capture_output=True, text=True).stdout)
