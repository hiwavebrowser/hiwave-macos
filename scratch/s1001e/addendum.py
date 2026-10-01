"""Append the 14:22 addendum to the digest and the PLAN, commit, push."""
import subprocess
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite'
add_d = ('\n**14:22 addendum (supersedes "open as #411" above):** **#411 MERGED** at head 104feb7 before the session '
         'stopped. So decision 1 is no longer a choice: wikipedia\'s text is drawn in the system font on develop '
         'until the generic-family fix lands, and that fix is this lane\'s next PR. #412 is open at d251d02 with CI '
         'still running.\n')
add_p = ('\n**2026-10-01 14:22: #411 is MERGED** (head 104feb7). #412 is open at d251d02, CI running, no review yet. '
         'Item (1) above is now "anything R1/R2 ask on #412"; item (2), the generic families, is the first engine '
         'work next session, and it is now urgent in a small way: develop draws `sans-serif` and unstyled text in '
         'the system font. The develop arm must be rebuilt (develop moved).\n')
for path, text in ((f'{HUB}/trench/digest-realsite.md', add_d), (f'{HUB}/trench/PLAN-realsite.md', add_p)):
    s = open(path).read()
    if text.strip() not in s:
        open(path, 'w').write(s.rstrip('\n') + '\n' + text)
msg = ('trench(realsite): digest 2026-10-01 14:22 addendum — #411 MERGED; #412 open, CI running\n\n'
       'Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>\n')
for cmd in (['git', 'add', '--', 'trench/digest-realsite.md', 'trench/PLAN-realsite.md', 'scratch/s1001e/addendum.py',
             'scratch/s1001e/finalize.py'],
            ['git', 'commit', '-q', '-m', msg],
            ['git', 'push', '-q', 'origin', 'atlas/trench-realsite'],
            ['git', 'log', '--oneline', '-1']):
    r = subprocess.run(cmd, cwd=HUB, capture_output=True, text=True)
    print(cmd[1], r.returncode, (r.stdout + r.stderr).strip()[-200:])
