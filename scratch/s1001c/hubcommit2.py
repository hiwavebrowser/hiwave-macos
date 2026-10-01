"""Addendum: the logical-borders receipt landed after the 08:25 digest. Append, commit, push the hub."""
import glob, subprocess
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite'
digest = '''
**08:33 addendum (supersedes "Branch pushed, NO PR yet" above):** the build finished at 08:21, so the receipt was taken and **#403 `atlas/rs-logical-borders` @ 04ebb59** is open. Campaign vs develop f16ad4e: all 26/26 and builtins 5/5 identical, ratchet output identical. On facebook the email box now paints all four border strips (develop: top and bottom only), read from both binaries' display lists. No points claimed.
'''
plan = '''
**2026-10-01 08:33: step (0) above is DONE.** `atlas/rs-logical-borders` is #403 (receipt identical, facebook's four border strips confirmed). The worktree `rs-dev-9f49a40` is still on that branch: detach it to origin/develop before using it as the develop arm. Open PRs from this lane: #401, #402, #403.
'''
for target, body in (('trench/digest-realsite.md', digest), ('trench/PLAN-realsite.md', plan)):
    cur = open(f'{HUB}/{target}').read()
    if body.strip() not in cur:
        open(f'{HUB}/{target}', 'a').write(('' if cur.endswith('\n') else '\n') + body)
paths = ['trench/digest-realsite.md', 'trench/PLAN-realsite.md']
for pat in ('scratch/s1001c/*.py', 'scratch/s1001c/*.md', 'scratch/s1001c/*.txt',
            'scratch/s0929e/camp-logborder-04ebb59/*.json', 'scratch/s0929e/camp-logborder-04ebb59/ratchet.txt'):
    paths += [p[len(HUB) + 1:] for p in glob.glob(f'{HUB}/{pat}')]
msg = ('trench(realsite): digest 2026-10-01 08:33 addendum — #403 (flow-relative border properties) opened, '
       'receipt identical\n\nCo-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>\n')
for cmd in (['git', 'add', '--'] + paths,
            ['git', 'commit', '-q', '-m', msg],
            ['git', 'push', '-q', 'origin', 'atlas/trench-realsite'],
            ['git', 'log', '--oneline', '-1'],
            ['git', 'status', '-sb', '--untracked-files=no']):
    r = subprocess.run(cmd, cwd=HUB, capture_output=True, text=True)
    print(cmd[1], r.returncode, (r.stdout + r.stderr).strip()[-300:])
