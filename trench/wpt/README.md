# WPT Tier-1 — Phase 0.5, units W0a + W0b

**What this is:** the checked-in list of Web Platform Tests HiWave measures itself against, pinned to a
frozen WPT SHA, and the runner that scores them through the real engine.

**Why it exists:** the campaign metric (pixel parity vs pinned Chrome 148) is *pinned-Chrome* parity — it
tops out at "matches Chrome, bugs included". As of 2026-07-29 it is **26/26 @ t15, avg 6.7 on committed
master**, i.e. saturated: every registry case passes with slack, so the meter can no longer tell an
improvement from a plateau. The north star in `trench/PLAN.md` was always absolute conformance. This is the
first honest brick of it.

**Pin:** `a6f29b0bedaf3f1edba7b6739127fe8e713bfcb3` (2026-07-29). Frozen for the campaign; re-pin only at a
campaign boundary, both seats in lockstep — the same rule as the CfT-148 pin.

**Design pin:** `../forensics/2026-07-15-wpt-phase05-GATE-OPEN.md` (path P0: thin reftest adapter over the
existing capture/pixel path — do not build a second engine host).
**Roadmap:** `LINE_BOX_WPT_ROADMAP.md` · **seed source:** `WPT_TIER1_SUBSET.md` (both on the hub).

## Run it

```
scripts/wpt_sync.sh
cargo build --profile parity -p parity-capture
python3 scripts/wpt_tier1.py
```

No Chrome is involved: a case passes when its test frame and reference frame match exactly. `status` uses
that rule alone. The campaign's rule (pixelmatch 0.1, 15%) is reported beside it as `campaign_diff_pct` and
`campaign_rule_counts`, and never sets a status; it needs `tools/parity_oracle/node_modules` and reads
`not_measured` without them.

## Layout

| Artifact | Role |
|---|---|
| `MANIFEST.json` | **Source of truth.** 30 test ids, their paths, buckets, support files, and which slice each one funds. |
| `scripts/wpt_sync.sh` | Materialises *only* the manifest paths into `third_party/wpt/` (gitignored). The manifest drives the checkout, never the reverse. |
| `scripts/wpt_tier1.py` | The runner. Aborts if the negative control does not go red. |
| `last-run.json` | Written by the runner (gitignored): counts, per-case rows, `develop_sha`, `oracle`. |

## The honesty note that gates this whole lane

`crates/rustkit-test`'s reftest path **does not render anything.** `run_comparison` normalises two HTML
strings and compares them — no parse, no style, no layout, no paint — and `layout.rs` passes unconditionally
when no `.expected` file is present. The January work order `.ai/work_orders/wpt-harness.json` is labelled
`status: completed`; its gates only checked that the crate builds.

**Nothing from `rustkit-test` may be reported as conformance.** That note is now also in the crate's own
module docs, where someone reading the code will hit it.

## Status words

- **ERROR** is the instrument: a capture failed, a frame was blank, `rel=match` disagrees with the manifest,
  or a page's local resource is not on disk. No pixels compared. Add a missing file to
  `MANIFEST.support_paths`.
- **SKIP** is a case the path cannot probe: `reftest-wait` (the frame is taken after load and nothing watches
  for the completion signal) and `fuzzy` annotations.
- An inline script without a wait is scored, with `--run-inline-scripts`.

## Known gaps

1. **On the macOS seat `wpt_sync.sh` has only run step by step.** The allowlist refuses the script as a
   whole, so T14 ran its git commands by hand in a fresh checkout. Its `--stdin` path list has not run.
2. **Seed is 30.** Grow it only when a campaign dig would have been clearer with a WPT id.
3. **No CI gate.** Per the pin §5 and Pete's standing rule, WPT must not gate PR merges until a floor is
   deliberately locked.

**An all-green run means the harness is lying**, same as the "13/13" smoke runner did (lie #6).
