"""PR body for the flaky font-resolve test fix: text plus develop's campaign table (test-only change)."""
import json
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch'


def load(scope):
    j = json.load(open(f'{HUB}/s0929e/camp-dev-f16ad4e/{scope}.json'))
    return j.get('timestamp', '')[:19], {r['case_id']: r['diff_pct'] for r in j['results']}, sum(
        1 for r in j['results'] if r.get('passed'))


ta, a, pa = load('all')
tb, b, pb = load('builtins')
rows = '\n'.join(f'| {k} | {a[k]:.4f} |' for k in sorted(a))
body = f'''## What

Two tests in `rustkit-layout`'s `text::font_resolve_tests` fail when the suite runs in parallel on a loaded machine. Found while taking the suite numbers for #401: develop **f16ad4e** gave **579/580 on 4 parallel runs out of 4** at machine load 14, the failure being one of these two; each passes when run alone.

The web-font generation is process-wide and several tests in the crate install a web-font set on their own threads.

- `a_new_web_font_set_invalidates_the_cache` shaped twice and asserted the second was a cache hit, unconditionally. An install on another test's thread between the two shapes made it a miss (`left: 2, right: 1`).
- `a_font_is_resolved_once_not_once_per_shape` allowed for one generation bump during its 200 shapes; there can be more than one.

## Change

Test code only, 27 lines.

- The first test judges the hit only when the generation held still across the pair, retrying up to 20 times: the pattern `a_new_web_font_set_invalidates_shaped_runs` in the same module already uses.
- The second reads the generation before and after its loop and allows two resolutions per bump, instead of a fixed allowance of one bump. With no bump it still requires exactly the cached count (2 for 200 shapes).

Neither test got weaker in the case it exists for: with the generation still, both assert what they asserted before.

## Tests

`cargo test -p rustkit-layout --lib`, parallel, 8 consecutive runs on this branch at machine load 14 to 15: **580/580 eight times.** Before the change, same machine and load: 579/580 four times out of four (two runs on develop f16ad4e, two on #401's branch).

## Campaign receipt

No binary changes: the diff is inside one `#[cfg(test)]` module, so a release build of this branch is develop f16ad4e's. The campaign was not re-run for it. Develop's own numbers, from the run taken this session for #401 (release binary built from f16ad4e with every workspace source touched first):

```
all:      {ta}  {pa}/{len(a)}  avg {sum(a.values())/len(a):.4f}
builtins: {tb}  {pb}/{len(b)}   avg {sum(b.values())/len(b):.4f}
```

<details><summary>Per-case diff_pct (all 26), develop f16ad4e = this branch</summary>

| case | diff_pct |
|---|---|
{rows}

</details>

🤖 Generated with [Claude Code](https://claude.com/claude-code)
'''
open(f'{HUB}/s1001c/pr-body-flake.md', 'w').write(body)
print(len(body), 'chars')
