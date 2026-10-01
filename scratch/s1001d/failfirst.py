"""Fail-first: put the fix branch's engine-level shadow pin (reads the display list as text) and its
frame test module (reads pixels) into develop's engine lib, run them there, restore.
usage: failfirst.py"""
import os, pathlib, subprocess, time
FIX = '/Users/petecopeland/Repos/.worktrees/rs-control-semantics/crates/rustkit-engine/src/lib.rs'
DEV_WT = '/Users/petecopeland/Repos/.worktrees/rs-dev-9f49a40'
DEV = DEV_WT + '/crates/rustkit-engine/src/lib.rs'
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite'
src = open(FIX).read()
orig = open(DEV).read()

PIN_START = "    /// The shadow of a rounded box is rounded, and its hole is the box's own\n"
PIN_END = '    #[test]\n    fn the_shadow_is_emitted_before_the_background() {'
pin = src[src.index(PIN_START):src.index(PIN_END)]

HEAD = ('// Needs a headless view: `cargo test -p rustkit-engine --features headless`.\n'
        '#[cfg(all(test, target_os = "macos", feature = "headless"))]\n')
MOD_START = HEAD + 'mod rounded_paint_frame_tests {'
MOD_END = HEAD + 'mod referrer_tests {'
mod = src[src.index(MOD_START):src.index(MOD_END)]

assert 'rounded_paint_frame_tests' not in orig and orig.count(PIN_END) == 1 and orig.count(MOD_END) == 1
open(DEV, 'w').write(orig.replace(PIN_END, pin + PIN_END).replace(MOD_END, mod + MOD_END))
now = time.time()
for f in pathlib.Path(DEV_WT, 'crates').rglob('*.rs'):
    os.utime(f, (now, now))
try:
    r = subprocess.run(['python3', f'{HUB}/scratch/s1001c/ctest_full.py', DEV_WT, '-p', 'rustkit-engine',
                        '--features', 'headless', '--lib', '--', 'windows_engine_pins',
                        'rounded_paint_frame_tests'], capture_output=True, text=True)
    print(r.stdout[-9000:], r.stderr[-2000:])
finally:
    open(DEV, 'w').write(orig)
    print(subprocess.run(['git', 'status', '--short'], cwd=DEV_WT, capture_output=True, text=True).stdout
          or 'develop worktree clean')
