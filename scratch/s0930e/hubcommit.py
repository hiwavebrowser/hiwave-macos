"""Commit this session's hub artifacts and push atlas/trench-realsite.
usage: python3 hubcommit.py <commit-message-file>"""
import glob, subprocess, sys
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite'
paths = ['trench/digest-realsite.md', 'trench/PLAN-realsite.md']
for pat in ('scratch/s0930e/*.py', 'scratch/s0930e/*.md', 'scratch/s0930e/commit-*.txt',
            'scratch/s0930e/probe/*.html', 'scratch/s0930e/probe/*.chrome.json',
            'scratch/s0930e/li-whatif-revert.png', 'trench/realsite/runs/20261001T0025Z-dev570e25d'):
    paths += [p[len(HUB) + 1:] for p in glob.glob(f'{HUB}/{pat}')]
for cmd in (['git', 'add', '--'] + paths,
            ['git', 'commit', '-q', '-F', sys.argv[1]],
            ['git', 'push', '-q', 'origin', 'atlas/trench-realsite'],
            ['git', 'log', '--oneline', '-1'],
            ['git', 'status', '-sb', '--untracked-files=no']):
    r = subprocess.run(cmd, cwd=HUB, capture_output=True, text=True)
    print(cmd[1], r.returncode, (r.stdout + r.stderr).strip()[-300:])
