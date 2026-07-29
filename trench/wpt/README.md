# WPT Tier-1 — Phase 0.5, unit W0a

**What this is:** the checked-in list of Web Platform Tests HiWave measures itself against, pinned to a
frozen WPT SHA. Nothing here reports a pass-rate yet.

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

## Layout

| Artifact | Role |
|---|---|
| `MANIFEST.json` | **Source of truth.** 14 test ids, their paths, buckets, and which line-box/IFC slice each one funds. |
| `scripts/wpt_sync.sh` | Materialises *only* the manifest paths into `third_party/wpt/` (gitignored). The manifest drives the checkout, never the reverse. |
| `last-run.json` | **Does not exist yet** — W0b writes it. Until it does, HiWave has **no** WPT pass-rate. |

## The honesty note that gates this whole lane

`crates/rustkit-test`'s reftest path **does not render anything.** `run_comparison` normalises two HTML
strings and compares them — no parse, no style, no layout, no paint — and `layout.rs` passes unconditionally
when no `.expected` file is present. The January work order `.ai/work_orders/wpt-harness.json` is labelled
`status: completed`; its gates only checked that the crate builds.

**Nothing from `rustkit-test` may be reported as conformance.** That note is now also in the crate's own
module docs, where someone reading the code will hit it.

## Known gaps (W0a ships with these open, deliberately)

1. **`wpt_sync.sh`'s network path has never been run.** `--dry-run` and `--check` were both exercised (28
   files = 14 tests + 14 refs; `--check` correctly fails with 28/28 missing on an empty tree). The actual
   sparse-checkout was not: this seat's Bash allowlist has no `git clone`, `git ls-remote`, or `curl`.
   Expect to fix something in that branch on its first real run.
2. **Seed is 14, not the pin's 25–32.** Only test/ref pairs whose **both** files appear verbatim in a pinned
   directory listing were admitted. 1A landed 5 of a targeted 15–18 (`css/css-text/white-space/` at this pin
   is entirely `break-spaces`, whose refs live outside the directory) and 1C landed 2 of 4–6. Growing the
   seed is a W0b task with the tree actually checked out — not a guess made from listings.
3. **Test→ref bindings are unverified.** WPT's authority is each test's `<link rel=match>`. The runner must
   read it and treat disagreement with `MANIFEST.json` as an *instrument error*, not a render diff.
4. **No CI gate.** Per the pin §5 and Pete's standing rule, WPT must not gate PR merges until a floor is
   deliberately locked. W0a adds no workflow at all.

## What "done" looks like next (W0b)

Render test and ref through the **same** `parity-capture` headless path the campaign uses, pixel-diff with the
existing oracle, and write `last-run.json` with `{pin, n, pass, fail, skip, rate, git_sha}`. Skip — with a
reason — for missing refs, unsupported `@supports`, or JS dependence; a skip is not a fail. **An all-green
first run means the harness is lying**, same as the "13/13" smoke runner did (lie #6).
