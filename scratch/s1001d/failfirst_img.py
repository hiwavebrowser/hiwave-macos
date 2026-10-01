"""Fail-first for the <img> border-radius pin: put it into develop's windows_engine_pins, run it there, restore.
usage: failfirst_img.py"""
import os, pathlib, subprocess, time
FIX = '/Users/petecopeland/Repos/.worktrees/rs-control-semantics/crates/rustkit-engine/src/lib.rs'
DEV_WT = '/Users/petecopeland/Repos/.worktrees/rs-dev-9f49a40'
DEV = DEV_WT + '/crates/rustkit-engine/src/lib.rs'
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite'
src = open(FIX).read()
orig = open(DEV).read()
START = '    /// `border-radius` on the `<img>` itself rounds the image: replaced\n'
END = "    /// The shadow of a rounded box is rounded, and its hole is the box's own\n"
pin = src[src.index(START):src.index(END)]
assert 'a_rounded_img_clips' not in orig and orig.count(END) == 1
open(DEV, 'w').write(orig.replace(END, pin + END))
now = time.time()
for f in pathlib.Path(DEV_WT, 'crates').rglob('*.rs'):
    os.utime(f, (now, now))
try:
    r = subprocess.run(['python3', f'{HUB}/scratch/s1001c/ctest_full.py', DEV_WT, '-p', 'rustkit-engine',
                        '--lib', '--', 'windows_engine_pins'], capture_output=True, text=True)
    print(r.stdout[-5000:], r.stderr[-1500:])
finally:
    open(DEV, 'w').write(orig)
    print(subprocess.run(['git', 'status', '--short'], cwd=DEV_WT, capture_output=True, text=True).stdout
          or 'develop worktree clean')
