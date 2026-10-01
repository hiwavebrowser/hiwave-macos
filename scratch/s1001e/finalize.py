"""Fill the last placeholders in digest.md, plan-note.md and commit-digest.txt."""
S = '/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/s1001e'


def sub(name, pairs):
    p = f'{S}/{name}'
    s = open(p).read()
    for old, new in pairs:
        assert s.count(old) == 1, (name, old)
        s = s.replace(old, new)
    open(p, 'w').write(s)


sub('digest.md', [
    ('__GENLIST__',
     '#412 below (a list like `Consolas, monospace` or `X, serif` fell through to the system font).**'),
    ('\n**Real-site A/B (asked for with this item',
     '- **#412 `atlas/rs-generic-in-list` @ d251d02** (base develop 6eb6f5f; opened 14:19, CI not finished and no '
     'review yet when the session stopped). `font-family: Consolas, monospace` on a Mac without Consolas was '
     'measured in the system font, which is proportional, and painted in Menlo at those advances; the same for '
     '`X, serif`. Layout treated the two keywords as font names when they came after another family. Chrome 148 on '
     'a 28-character line: 268.84 and 199.52; develop 220.78 for both; with the fix 269.72 and 199.51.\n'
     '  - **Test:** one layout test, which **fails with the new code disabled** (220.78 against 269.72) and passes '
     'with it. rustkit-layout 584/584. Full engine suite not run locally.\n'
     '  - **Campaign vs develop 6eb6f5f:** all 26/26, builtins 5/5, micro 13/13 identical; ratchet output '
     'identical; all 26 frames byte-identical. Expected: no campaign page ends a list of missing families in '
     '`serif` or `monospace`. No real-site A/B was taken for it (cap).\n'
     '\n**Real-site A/B (asked for with #411'),
    ('**PR opened (Prometheus R1 + Cursor R2; not mine to merge):**',
     '**PRs opened (Prometheus R1 + Cursor R2; not mine to merge):**'),
])
sub('plan-note.md', [
    ('__GENLIST__',
     '**#412** (`atlas/rs-generic-in-list` @ d251d02, opened 14:19): `serif` / `monospace` after missing families '
     'resolve as the generic in layout; receipt identical. Worktrees: `rs-control-semantics` is on #411\'s branch, '
     '`rs-light-dark` on #412\'s, `rs-dev-9f49a40` is detached at develop 6eb6f5f.'),
    ('__GENLIST_ASK__', ' or #412'),
])
sub('commit-digest.txt', [('__GENLIST_SHORT__', 'opened as #412, receipt identical')])
print('ok')
