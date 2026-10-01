"""Commit this session's hub artifacts and push atlas/trench-realsite."""
import glob, subprocess
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite'
paths = ['trench/digest-realsite.md', 'trench/PLAN-realsite.md']
for pat in ('scratch/s1001/*.py', 'scratch/s1001/*.html', 'scratch/s1001/*.md', 'scratch/s1001/*.txt',
            'scratch/s1001/controls-*.png', 'scratch/s1001/controls-chrome.json', 'scratch/s1001/settings-select-strip.png',
            'scratch/s1001/woff-e2e-dev.png', 'scratch/s1001/Ahem.*',
            'scratch/s0929e/camp-dev-9c701ab/*.json', 'scratch/s0929e/camp-dev-9c701ab/ratchet.txt',
            'scratch/s0929e/camp-controls-wip/*.json', 'scratch/s0929e/camp-controls-wip/ratchet.txt'):
    paths += [p[len(HUB) + 1:] for p in glob.glob(f'{HUB}/{pat}')]
for cmd in (['git', 'add', '--'] + paths,
            ['git', 'commit', '-q', '-F', 'scratch/s1001/commit-digest.txt'],
            ['git', 'push', '-q', 'origin', 'atlas/trench-realsite'],
            ['git', 'log', '--oneline', '-1'],
            ['git', 'status', '-sb', '--untracked-files=no']):
    r = subprocess.run(cmd, cwd=HUB, capture_output=True, text=True)
    print(cmd[1], r.returncode, (r.stdout + r.stderr).strip()[-300:])
