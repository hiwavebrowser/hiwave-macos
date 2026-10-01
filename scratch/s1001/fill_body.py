"""Fill __TABLE__ and __SUITES__ in the controls PR body."""
import json
D = '/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch'
body = f'{D}/s1001/pr-body-controls.md'


def load(arm):
    j = json.load(open(f'{D}/s0929e/camp-{arm}/all.json'))
    return {r['case_id']: r['diff_pct'] for r in j['results']}


a, b = load('dev-9c701ab'), load('controls-wip')
rows = '\n'.join(f'| {k} | {a[k]:.4f} | {b[k]:.4f} |' for k in sorted(a))
suites = ('Suites on head 90be88d: **rustkit-layout 580/580, rustkit-renderer 90/90.** rustkit-engine: the 23 control, '
          'form and input tests pass serially (`--test-threads=1`), including the pre-existing '
          '`a_select_shows_its_selected_option`. The full engine suite run in parallel at machine load 13 to 18 gave '
          '231 passed and 38 failed, **every one of the 38 the `GPU test guard: waited …` timeout** '
          '(`lib.rs:105`, tests queueing for the GPU), none an assertion and none in a control test. '
          'A full serial run did not fit the session, so CI is the full-suite evidence for this PR.')
s = open(body).read().replace('__TABLE__', rows).replace('__SUITES__', suites)
open(body, 'w').write(s)
print(s.count('|'), 'pipes;', len(s), 'chars')
