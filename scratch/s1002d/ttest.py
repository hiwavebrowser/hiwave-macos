"""Touch every crate source in a worktree (the shared target judges freshness by mtime and has run
another branch's test binary before), then cargo test there through s1002a/cerr.py.
usage: python3 ttest.py <worktree name> <cargo test args...>"""
import os, pathlib, subprocess, sys, time
wt = '/Users/petecopeland/Repos/.worktrees/' + sys.argv[1]
now = time.time()
for p in pathlib.Path(wt, 'crates').rglob('*.rs'):
    os.utime(p, (now, now))
sys.exit(subprocess.run(['python3', '/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/s1002a/cerr.py', wt]
                        + sys.argv[2:]).returncode)
