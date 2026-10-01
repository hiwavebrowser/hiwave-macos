"""Fail-first: append the fix branch's control_semantics_tests module to develop's engine lib.rs,
run it there, then restore the file. usage: failfirst.py"""
import subprocess
FIX = '/Users/petecopeland/Repos/.worktrees/rs-control-semantics/crates/rustkit-engine/src/lib.rs'
DEV_WT = '/Users/petecopeland/Repos/.worktrees/rs-dev-9f49a40'
DEV = DEV_WT + '/crates/rustkit-engine/src/lib.rs'
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite'
src = open(FIX).read()
mod = src[src.index('#[cfg(test)]\nmod control_semantics_tests {'):]
orig = open(DEV).read()
assert 'control_semantics_tests' not in orig
open(DEV, 'w').write(orig + '\n' + mod)
try:
    r = subprocess.run(['python3', f'{HUB}/scratch/s0930/ctest.py', DEV_WT, '--touch', '-p', 'rustkit-engine', '--lib',
                        '--', 'control_semantics'], capture_output=True, text=True)
    print(r.stdout[-6000:], r.stderr[-2000:])
finally:
    open(DEV, 'w').write(orig)
    print(subprocess.run(['git', 'status', '--short'], cwd=DEV_WT, capture_output=True, text=True).stdout or 'develop worktree clean')
