"""Append this session's digest section and PLAN note, commit them with the session's receipts, push the hub.
usage: hubcommit.py <digest.md> <plan.md> <commit subject>"""
import glob, subprocess, sys
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite'
digest, plan, subject = open(sys.argv[1]).read(), open(sys.argv[2]).read(), sys.argv[3]
for target, body in (('trench/digest-realsite.md', digest), ('trench/PLAN-realsite.md', plan)):
    cur = open(f'{HUB}/{target}').read()
    if body.strip() not in cur:
        open(f'{HUB}/{target}', 'a').write(('' if cur.endswith('\n') else '\n') + '\n' + body.strip() + '\n')
paths = ['trench/digest-realsite.md', 'trench/PLAN-realsite.md']
for pat in ('scratch/s1001d/*.py', 'scratch/s1001d/*.md', 'scratch/s1001d/*.txt', 'scratch/s1001d/*.html',
            'scratch/s1001d/quad.png', 'scratch/s1001d/alpha.png', 'scratch/s1001d/dev.png',
            'scratch/s1001d/fix.png', 'scratch/s1001d/chrome.png', 'scratch/s1001d/dev-fix-chrome.png',
            'scratch/s1001d/imgr.png', 'scratch/s1001d/ab-*.png',
            'scratch/s0929e/camp-dev-473a047/*.json', 'scratch/s0929e/camp-dev-473a047/ratchet.txt',
            'scratch/s0929e/camp-rsc-wip/*.json', 'scratch/s0929e/camp-rsc-wip/ratchet.txt',
            'scratch/s0929e/camp-imgr-a739e7d/*.json', 'scratch/s0929e/camp-imgr-a739e7d/ratchet.txt'):
    paths += [p[len(HUB) + 1:] for p in glob.glob(f'{HUB}/{pat}')]
msg = subject + '\n\nCo-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>\n'
for cmd in (['git', 'add', '--'] + paths,
            ['git', 'commit', '-q', '-m', msg],
            ['git', 'push', '-q', 'origin', 'atlas/trench-realsite'],
            ['git', 'log', '--oneline', '-1'],
            ['git', 'status', '-sb', '--untracked-files=no']):
    r = subprocess.run(cmd, cwd=HUB, capture_output=True, text=True)
    print(cmd[1], r.returncode, (r.stdout + r.stderr).strip()[-300:])
