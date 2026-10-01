"""Fail-first: put the fix branch's five engine-level corner pins (they read the display list as text,
so they compile on either tree) into develop's windows_engine_pins module, run them there, restore.
usage: failfirst.py"""
import subprocess
FIX = '/Users/petecopeland/Repos/.worktrees/rs-control-semantics/crates/rustkit-engine/src/lib.rs'
DEV_WT = '/Users/petecopeland/Repos/.worktrees/rs-dev-9f49a40'
DEV = DEV_WT + '/crates/rustkit-engine/src/lib.rs'
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite'
START = '    /// The radius of the one rounded rect a 200x100 box with `style_decls`\n'
END = '    // ── box-shadow paint order (#74) ──'
src = open(FIX).read()
block = src[src.index(START):src.index(END)]
orig = open(DEV).read()
assert 'fn painted_radius' not in orig and orig.count(END) == 1
open(DEV, 'w').write(orig.replace(END, block + END))
# The shared target judges path crates by mtime: without this, develop's crates link the fix's rlibs.
import os, pathlib, time
now = time.time()
for f in pathlib.Path(DEV_WT, 'crates').rglob('*.rs'):
    os.utime(f, (now, now))
try:
    r = subprocess.run(['python3', f'{HUB}/scratch/s1001c/ctest_full.py', DEV_WT, '-p', 'rustkit-engine', '--lib',
                        '--', 'windows_engine_pins'], capture_output=True, text=True)
    print(r.stdout[-9000:], r.stderr[-2000:])
finally:
    open(DEV, 'w').write(orig)
    print(subprocess.run(['git', 'status', '--short'], cwd=DEV_WT, capture_output=True, text=True).stdout
          or 'develop worktree clean')
