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
python3 scripts/wpt_coverage.py
```

That syncs (`scripts/wpt_sync.sh`), builds (`cargo build --profile parity -p parity-capture`) and runs
(`scripts/wpt_tier1.py`), and ends on the per-area table. About 2 minutes after the build, at 4 cases at a time
(`--jobs`). `--area <name>` runs one area and `--case <id>` one case; neither writes `last-run.json`.

No Chrome is involved: a case passes when its test frame and reference frame match exactly. `status` uses
that rule alone. The campaign's rule (pixelmatch 0.1, 15%) is reported beside it as `campaign_diff_pct` and
`campaign_rule_counts`, and never sets a status; it needs `tools/parity_oracle/node_modules`, and a case it
could not measure says why in `campaign_not_measured`.

The runner reads `<CARGO_TARGET_DIR or target>/parity/parity-capture`. `scripts/parity_test.py` builds and runs
the `--release` binary instead, so a campaign number and a WPT count on one commit come from two builds of the
same source.

Every match is then rendered again broken on purpose: once with all its author style removed, and once per
`mutations` entry in the manifest (each replaces text in the test page). If any of those still matches the
reference, the case is UNPROVEN, `unproven_passes` in `last-run.json` says which probe, and it is not in the
pass count. A mutation that no longer applies says so and is UNPROVEN too.

## Layout

| Artifact | Role |
|---|---|
| `MANIFEST.json` | **Source of truth.** Test ids, paths, buckets, support files. The 30 with no `area` are the W0b seed. The rest are coverage (T15), and `maps_to` is the Z plan row a FAIL goes to. |
| `scripts/wpt_import.py` | Chooses the coverage entries and writes them into the manifest, by hand and only when an area changes. Neither sync nor run needs it. |
| `scripts/wpt_coverage.py` | The one command above. |
| `scripts/wpt_sync.sh` | Materialises *only* the manifest paths into `third_party/wpt/` (gitignored). The manifest drives the checkout, never the reverse. |
| `scripts/wpt_tier1.py` | The runner. Aborts if the negative control does not go red. |
| `last-run.json` | Written by the runner (gitignored): counts, `areas`, per-case rows, `develop_sha`, `oracle`, `unproven_passes`. |

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
  `MANIFEST.support_paths`. A missing resource outranks SKIP.
- **SKIP** is a case the path cannot probe: `reftest-wait` (the frame is taken after load and nothing watches
  for the completion signal), `fuzzy` annotations, and an external `<script src>` (`--html-file` runs inline
  scripts only). The importer takes none of these, so SKIP is seed-only today.
- **UNPROVEN** matched its reference and also matched when broken on purpose. Scored, not a pass.
- An inline script without a wait is scored, with `--run-inline-scripts`. A script that throws is an ERROR.

## Known gaps

1. **The macOS lane seat cannot run `wpt_sync.sh` as a script.** Its allowlist refuses it, so lane sessions run
   the git steps by hand. The script itself ran end to end in the #844 R1, `--stdin` path list included.
2. **The seed stays 30; coverage grows by area.** An area takes reftests the runner can score, evenly spread
   down to a cap, so it is a sample of the area and not its pass rate. `maps_to` is by area: a FAIL has not been
   reduced, and may turn out to belong to another row.
3. **No CI gate.** Per the pin §5 and Pete's standing rule, WPT must not gate PR merges until a floor is
   deliberately locked.

**An all-green run means the harness is lying**, same as the "13/13" smoke runner did (lie #6).
