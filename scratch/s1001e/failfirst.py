"""Run the new test with the fix disabled (the new arm made unreachable), then restore the file.
usage: failfirst.py <worktree>"""
import subprocess, sys
wt = sys.argv[1]
p = f'{wt}/crates/rustkit-layout/src/text.rs'
orig = open(p).read()
on = 'else if lower == "serif" || lower == "monospace" {'
assert orig.count(on) == 1
try:
    open(p, 'w').write(orig.replace(on, 'else if false && (lower == "serif" || lower == "monospace") {'))
    r = subprocess.run(['cargo', 'test', '-p', 'rustkit-layout', '--lib',
                        'a_generic_after_missing_families_resolves_as_the_generic'],
                       cwd=wt, capture_output=True, text=True)
    out = r.stdout + r.stderr
    keep = [l for l in out.splitlines() if 'test result' in l or 'panicked' in l or 'left:' in l or 'right:' in l]
    print('\n'.join(keep[-8:]))
finally:
    open(p, 'w').write(orig)
print('restored:', open(p).read() == orig)
