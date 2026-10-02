"""Fail-first for the background shorthand reset: splice the fix branch's test into develop's engine
test module (non-test code untouched), run it, restore the file.
usage: failfirst_bg.py <develop-worktree> <fix-worktree>"""
import os, subprocess, sys

dev, fix = sys.argv[1:3]
ENV = dict(os.environ, CARGO_TARGET_DIR='/Users/petecopeland/Repos/.worktrees/rs-dev-bf806c5/target')
REL = 'crates/rustkit-engine/src/lib.rs'
START = '    #[test]\n    #[cfg(target_os = "macos")]\n    fn the_background_shorthand_resets_the_colour_it_does_not_name'
END = '    #[test]\n    fn split_important_strips_the_flag'

f = open(f'{fix}/{REL}', newline='').read()
orig = open(f'{dev}/{REL}', newline='').read()
i = f.index(START)
block = f[i:f.index(END, i)]
assert END in orig and START not in orig
try:
    open(f'{dev}/{REL}', 'w', newline='').write(orig.replace(END, block + END, 1))
    r = subprocess.run(['cargo', 'test', '-p', 'rustkit-engine', '--lib', 'the_background_shorthand'],
                       cwd=dev, env=ENV, capture_output=True, text=True)
    out = r.stdout + r.stderr
    for l in out.splitlines():
        if ('test result' in l or 'panicked' in l or l.startswith('error') or l.startswith('test ')
                or 'left' in l or 'right' in l or 'assertion' in l):
            print(l[:260], flush=True)
    print('rc', r.returncode)
finally:
    open(f'{dev}/{REL}', 'w', newline='').write(orig)
print('restored:', open(f'{dev}/{REL}', newline='').read() == orig, flush=True)
