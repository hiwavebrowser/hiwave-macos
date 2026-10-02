"""Fail-first for the shorthand-layers fix: splice its first two tests into the base branch's engine test
module (non-test code untouched), run them, restore the file. The third test calls a function the fix adds,
so it cannot compile on the base and is left out.
usage: failfirst_layers.py <base-worktree> <fix-worktree>"""
import os, subprocess, sys

dev, fix = sys.argv[1:3]
ENV = dict(os.environ, CARGO_TARGET_DIR='/Users/petecopeland/Repos/.worktrees/rs-dev-bf806c5/target')
REL = 'crates/rustkit-engine/src/lib.rs'
START = ('    #[test]\n    #[cfg(target_os = "macos")]\n'
         '    fn the_background_shorthand_sets_each_layers_position_size_and_repeat')
STOP = '    #[test]\n    fn a_shorthand_layers_url_is_taken_whole'
ANCHOR = '    #[test]\n    fn split_important_strips_the_flag'

f = open(f'{fix}/{REL}', newline='').read()
orig = open(f'{dev}/{REL}', newline='').read()
i = f.index(START)
block = f[i:f.index(STOP, i)]
assert ANCHOR in orig and START not in orig
try:
    open(f'{dev}/{REL}', 'w', newline='').write(orig.replace(ANCHOR, block + ANCHOR, 1))
    for name in ('the_background_shorthand_sets_each_layers', 'a_background_position_reads_its_keywords'):
        r = subprocess.run(['cargo', 'test', '-p', 'rustkit-engine', '--lib', name],
                           cwd=dev, env=ENV, capture_output=True, text=True)
        out = r.stdout + r.stderr
        for l in out.splitlines():
            if ('test result' in l or 'panicked' in l or l.startswith('error') or l.startswith('test ')
                    or l.strip().startswith(('left', 'right')) or 'assertion' in l):
                print(l[:260], flush=True)
        print('rc', r.returncode, flush=True)
finally:
    open(f'{dev}/{REL}', 'w', newline='').write(orig)
print('restored:', open(f'{dev}/{REL}', newline='').read() == orig, flush=True)
