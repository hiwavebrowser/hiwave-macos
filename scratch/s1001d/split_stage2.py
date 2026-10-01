"""Stage 2 of the two-commit split. `save`: copy the pending layout and engine lib.rs aside and check out the
committed ones (so commit 1's tree can be tested alone). `restore`: put those two and the full renderer lib.rs back.
usage: split_stage2.py save|restore"""
import shutil, subprocess, sys
WT = '/Users/petecopeland/Repos/.worktrees/rs-control-semantics'
S = '/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/s1001d'
FILES = {'crates/rustkit-layout/src/lib.rs': 'layout-lib-full.rs', 'crates/rustkit-engine/src/lib.rs': 'engine-lib-full.rs'}
if sys.argv[1] == 'save':
    for rel, name in FILES.items():
        shutil.copy2(f'{WT}/{rel}', f'{S}/{name}')
    print(subprocess.run(['git', 'checkout', 'HEAD', '--', *FILES], cwd=WT, capture_output=True, text=True))
else:
    for rel, name in FILES.items():
        shutil.copyfile(f'{S}/{name}', f'{WT}/{rel}')
    shutil.copyfile(f'{S}/renderer-lib-full.rs', f'{WT}/crates/rustkit-renderer/src/lib.rs')
print(subprocess.run(['git', 'status', '--short'], cwd=WT, capture_output=True, text=True).stdout)
