"""Fail-first: put the fix branch's web_font_format_tests module and the WOFF/WOFF2 fixtures into
develop's tree, run the test there, then restore the tree. usage: failfirst.py"""
import os, shutil, subprocess
FIX_WT = '/Users/petecopeland/Repos/.worktrees/rs-control-semantics'
DEV_WT = '/Users/petecopeland/Repos/.worktrees/rs-dev-9f49a40'
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite'
ENG = '/crates/rustkit-engine/src/lib.rs'
FIXT = '/crates/rustkit-text/tests/fixtures/'
src = open(FIX_WT + ENG).read()
start = src.index('// Needs a headless view: `cargo test -p rustkit-engine --features headless`.\n'
                  '#[cfg(all(test, target_os = "macos", feature = "headless"))]\nmod web_font_format_tests {')
end = src.index('// Needs a headless view', start + 10)
mod = src[start:end]
orig = open(DEV_WT + ENG).read()
assert 'web_font_format_tests' not in orig
open(DEV_WT + ENG, 'w').write(orig + '\n' + mod)
for f in ('Ahem.woff', 'Ahem.woff2'):
    shutil.copy2(FIX_WT + FIXT + f, DEV_WT + FIXT + f)
try:
    r = subprocess.run(['python3', f'{HUB}/scratch/s0930/ctest.py', DEV_WT, '--touch', '-p', 'rustkit-engine',
                        '--features', 'headless', '--lib', '--', 'web_font_format_tests'],
                       capture_output=True, text=True)
    print(r.stdout[-6000:], r.stderr[-2000:])
finally:
    open(DEV_WT + ENG, 'w').write(orig)
    for f in ('Ahem.woff', 'Ahem.woff2'):
        os.remove(DEV_WT + FIXT + f)
    print(subprocess.run(['git', 'status', '--short'], cwd=DEV_WT, capture_output=True, text=True).stdout
          or 'develop worktree clean')
    print(subprocess.run(['git', 'rev-parse', '--short', 'HEAD'], cwd=DEV_WT, capture_output=True, text=True).stdout)
