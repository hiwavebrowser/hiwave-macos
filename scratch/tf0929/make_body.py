import json

W = '/Users/petecopeland/Repos/.worktrees'
head = json.load(open(f'{W}/rs-dev-bf806c5/parity-baseline/parity_test_results.json'))
dev = json.load(open(f'{W}/rs-dev-c2772b1/parity-baseline/parity_test_results.json'))
hv = {r['case_id']: r['pixel']['diffPercent'] for r in head['results']}
dv = {r['case_id']: r['pixel']['diffPercent'] for r in dev['results']}
avg = lambda d: sum(d.values()) / len(d)
same = sum(1 for c in hv if abs(hv[c] - dv.get(c, -1)) < 1e-9)
rows = '\n'.join(f'| `{c}` | {hv[c]:.4f} | {dv[c]:.4f} |' for c in hv)

body = f"""## What

The individual transform properties, **`translate`, `rotate` and `scale`** (css-transforms-2 §5), plus the **var() bug that hid them on real sites**.

1. **f3a1c68: the properties.** RustKit dropped all three. Each now cascades on its own (`ComputedStyle::{{translate,rotate,scale}}`, where `None` is `none`). `ComputedStyle::effective_transform()` composes them in the order §6 gives: translate, then rotate, then scale, then `transform`. The painter (`PushTransform` in rustkit-layout) and the engine's rect export (`own_transform_affine`) both read it, so they can't disagree. A nonzero z, a `rotate` about x/y, or a scale z other than 1 is 3D, and the declaration is dropped rather than painted flat.
2. **1d1c972: token-safe `var()` substitution.** Substitution is token-level (CSS Variables 1 §3), but `substitute_css_vars` spliced text. Tailwind v4 writes every translate/scale utility as `translate:var(--tw-translate-x)var(--tw-translate-y)`, with no space, so `0` and `-200%` became the single invalid `0-200%`. Without this, (1) never fires on a Tailwind v4 site. A substituted value now gets a space only on a side where it would otherwise run together (ident, number, dimension or hash characters on both sides). `calc(var(--n)*2px)` and `rgb(var(--r),...)` come out byte-identical to before (pinned).

## Why (real-site board)

shopify's "Skip to Content" link is `translate-y-[-200%]`. Chrome parks it above the viewport, and RustKit painted it top-left over the header.

**shopify LOOKS RIGHT: develop 25.9% / 25.9% → this PR 24.0% / 24.2%** (2 interleaved rounds, runs `tf0929b-{{dev,fix}}-{{0,1}}`, Chrome-vs-Chrome 0.0%). The skip link now paints under `translate(0,-88px)` (-200% of its 44 px), as in Chrome. The page's display list goes from 9 `push_transform` to 71. No check flips: shopify's LOOKS RIGHT fails at 15% either way, and READABLE/LOADS are unchanged.

A full-board A/B is not in this body: the seat's board slot this session went to #346's receipt. I'll add it as a comment when it runs.

## Tests

`individual_transform_tests` has 6 pins:
- parse (`10px 20%`, `90deg`, `z 45deg`, `50%`, `2 3`);
- `none` resets, and invalid or 3D values leave the previous value alone;
- composition order;
- `translate: 0 -200%` moves a 20 px box by -40 through a full style+layout pass;
- adjacent substitutions stay separate tokens;
- Tailwind v4's exact rule shape moves the box.

The layout pin fails without f3a1c68 (translate arm disabled). The last two fail without 1d1c972 (`fuses` disabled).

- rustkit-engine lib `--features headless --test-threads=1`: **270/270**.
- rustkit-css + rustkit-layout: all pass except `text::font_resolve_tests::a_new_web_font_set_invalidates_the_cache`. That's the known flaky shared-cache test: it passes alone and also flakes on #347's merge today.

**Campaign receipt (gate 5):** `scripts/parity_test.py` (all scopes), run {head['timestamp']}, local on head 1d1c972. **{head['passed']}/{head['passed'] + head['failed']} passed, avg diff_pct {avg(hv):.4f}%** vs develop 062f73a {avg(dv):.4f}% (run {dev['timestamp']}, same seat). **{same}/{len(hv)} cases identical.**

**CI gates locally** (`ratchet_local.py`, Gate A + Gate B + ratchet): output **byte-identical** to the same script on develop 062f73a. Both exit 2, which is develop's existing state; this PR doesn't change it.

**WPT tier 1:** not run. `third_party/wpt` isn't synced in this worktree (54/54 manifest paths absent). No WPT-facing surface changed beyond the two properties.

<details><summary>Per-case diff_pct (26 cases)</summary>

| case | this PR (1d1c972) | develop (062f73a) |
|---|---|---|
{rows}

</details>

🤖 Generated with [Claude Code](https://claude.com/claude-code)
"""
open('/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/tf0929/pr-body.md', 'w').write(body)
print(same, len(hv))
