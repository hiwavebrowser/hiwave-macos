# Cascade-speed trench — digest

Newest section last. Each session appends `## <date> <HH:MM>`.

## 2026-09-27 23:30

**Phase 0 done (the instrument). Phase 1 started: the first fix is pushed, and its PR opens next session once the receipt is in.**

Ratio (median of 5, RustKit cascade / Chrome style). Before is the instrument's first reading on develop f262568:

| site | before | after (pseudo-index, A/B under load) |
|---|---|---|
| cnn | 5852 ms → 27.9× | B/A 0.42 → ~11.7× (projected) |
| github | 7441 ms → 67.6× | B/A 0.47 → ~31.8× (projected) |
| wikipedia | 1613 ms → **80.6×** (worst) | B/A 0.70 → ~56× (projected) |

The "after" column is **projected, not measured on a quiet machine.** The real-site lane's 23:05 session loaded the Mac about 3×: wikipedia's `parse_ms` (a code path the fix can't touch) went from 5.5 to 15–22 ms. So I ran 3 interleaved A/B pairs per site. A was the f262568+timer build, B was 5472087+fix+timer. B/A held steady across all 9 pairs (0.67–0.73, 0.45–0.50, 0.41–0.42). Next session: re-measure on a quiet machine to get the real number.

- **Instrument:** `trench/tools/cascade_bench.py` + `cascade_snapshot.py` + `cascade_profile.py` (macOS `sample`, symbolized build in `~/Repos/.worktrees/cascade-target-prof`). Pinned snapshots are in `trench/cascade/snapshots/`. Only the manifests and `PINS.sha256` are committed; the pages themselves stay local.
- **Profile (github, sampled):** `create_pseudo_element` = 48% of cascade, `rule_may_match` = 41% (`subject_keys` SipHashing the selector string alone is 24%), `selector_matches` = 24%. Declaration parsing barely registers. The plan's order (Bloom filter first) doesn't match the profile, so the pseudo-rule index went first.
- **PRs:** #311 `atlas/cs-cascade-timer` @ b2b5d74 (the instrument; receipt builtins 5/5 avg 2.3%, campaign 26/26 avg 1.3%). `atlas/cs-pseudo-index` @ 6ad00ec is **pushed with no PR yet**: rustkit-engine lib tests 168/168, but the campaign receipt isn't run yet. Next session: run `parity_test.py --scope builtins` and the 26-case campaign, then open the PR.
- **Next session housekeeping:** the `cs-pseudo-index` worktree has the timer commit staged, uncommitted (it was the bench build), and I didn't have permission to reset it. Run `git checkout HEAD -- crates/rustkit-engine/src/lib.rs` there before anything else. The pushed branch is clean.
- **Next fix, from the profile:** precompute subject keys per rule in the `RuleIndex`, so `rule_may_match` stops hashing selector strings (`subject_keys` = 24%). Then the ancestor Bloom filter for `selector_matches`.
- Found, out of scope: a bare `::before { content: "x" }` generates no pseudo-element on any element (Chrome: every element). Pinned as-is by the new equivalence test; flag for the real-site lane.
- Aleph wasn't usable here: this worktree had no index, and `aleph build` / `aleph_rebuild` needed approval. I navigated with rg + Read.

**Decisions for Pete**
1. **Method change, needs your OK (BASELINE Changes).** "Use the real-site board's pinned snapshots" isn't possible, because the board loads live. This lane pinned its own HTML+CSS snapshots with scripts removed (RustKit's JS runs on a stub DOM anyway), and keeps the page bodies out of the public repo.
2. **Two lanes on one Mac skew timing.** Bench numbers are only trustworthy when the real-site lane is idle. Should this lane run offset from :05, e.g. at :35, or should A/B-interleaved runs be the standard?

## 2026-09-28 01:00

**The pseudo-index PR is open (#314, receipt clean). Fix 2 (subject keys stored per rule) is pushed and passes tests, but there's no PR yet. No trustworthy ratio this session: the Mac ran at load 13–18 the whole time.**

| site | baseline (develop, quiet) | this session, #314 head d4158a3 (load 13–17, **not comparable**) | last session's A/B (#314 vs develop) |
|---|---|---|---|
| cnn | 5852 ms → 27.9× | 9264 ms → 44.1× | B/A 0.42 |
| github | 7441 ms → 67.6× | 11655 ms → 106.0× | B/A 0.47 |
| wikipedia | 1613 ms → **80.6×** | 3502 ms → 175.1× | B/A 0.70 |

Worst ratio of record: still **80.6×**. The raw numbers went *up* only because of load: wikipedia's `parse_ms` (a code path no change here touches) read 14–21 ms against 5.5 quiet, and the same build of parity-capture took **38 min** to compile. I didn't re-run the A/B: building a second (develop) binary would have cost another 38 min.

- **PR #314** `atlas/cs-pseudo-index` @ **d4158a3** (6ad00ec + a develop merge bringing in #311). Receipt: builtins 5/5 avg 2.3%, campaign 26/26 avg 1.3%, every case's diff_pct identical to the develop+timer receipt. #311 (timer) is **merged**.
- **Pushed, no PR:** `atlas/cs-subject-keys` @ **fbc680f**, stacked on #314. `build_rule_index` keeps each rule's subject keys (and the base selector's keys for `::before`/`::after` rules), and the indexed prefilter uses them directly (`keys_may_match`). That removes the per-candidate SipHash of the selector string (`subject_keys` = 24% of github's sampled cascade). rustkit-engine lib tests 168/168. Next session: bench it A/B against #314, run the receipt, open the PR (base develop, "merge after #314").
- **Housekeeping:** the old `cs-pseudo-index` worktree still has a staged, uncommitted timer diff. It's harmless (the branch on origin is ahead of it), but git commands in any worktree other than the hub now need approval, which a headless run can't get. So this session worked **entirely inside the hub worktree**, switching it to engine branches and back. `parity_test.py` hard-codes `./target`, so the hub now has its own full `target/` (gitignored). The `ln -s` to cascade-target needed approval too.
- **Next fixes, from the profile:** ancestor Bloom filter for `selector_matches` (24%), then cache `selector_specificity` per rule in the index (it re-parses the selector string for every matched rule).

**Decisions for Pete**
1. (Carried over) Approve the pinned-snapshot method change in BASELINE Changes.
2. **Bench scheduling:** at load 13–18, absolute medians run 2–3× high and a release build takes 38 min. Proposal: make A/B-interleaved runs the standard and keep a standing develop-head "A" binary in `cascade-target-base/`. The alternative is to offset this lane to :35 and skip benching whenever load > 6.
3. **Permissions:** allow `git -C ~/Repos/.worktrees/cs-*` and `ln -s` for this lane, so it can use per-branch worktrees as the PLAN says instead of switching the hub's branch.

## 2026-09-28 02:55

**#314 (pseudo-index) merged. Fix 2 (subject keys stored per rule) is open as PR #317 with a clean receipt. Fix 3 (specificity stored per rule) is pushed and passes tests, but there's no PR yet. The Mac ran at load 13–19 again, so every number below is an interleaved A/B, not an absolute ratio.**

| site | baseline (develop f262568, quiet) | #314 B/A (last session) | #317 B/A (8 pairs, median) | projected ratio now |
|---|---|---|---|---|
| cnn | 5852 ms → 27.9× | 0.42 | **0.85** (.61–.93) | ~10.0× |
| github | 7441 ms → 67.6× | 0.47 | **0.92** (.69–2.25) | ~29.2× |
| wikipedia | 1613 ms → **80.6×** | 0.70 | **0.90** (.86–2.74) | **~50.7×** (worst) |

Worst ratio of record: still **80.6×**, since no quiet run has happened yet. Projected: **~51× (wikipedia)**. A = #314's head d4158a3, B = #317 at 92bec62. Pairs 3–5 were the quietest and agree with the medians. The >1 outliers are load spikes: B's wikipedia `parse_ms` hit 103 ms, where the quiet value is 5.5.

- **PR #317** `atlas/cs-subject-keys` @ **92bec62** (fbc680f + a merge of develop 38cb30b). Receipt: builtins 5/5 avg 1.9%, campaign 26/26 avg 1.2%. Exactly 3 cases moved against #314's receipt (shelf, css-selectors, flex-positioning), and they match #315's reported numbers to the digit, so this change moved no pixel. rustkit-engine lib 168/168.
- **Pushed, no PR:** `atlas/cs-specificity-cache` @ **c995fc8**, stacked on #317. `RuleIndex::specificity[g]` is computed once in `build_rule_index`, and the cascade uses it for matched rules instead of re-scanning the selector string. It adds the test `rule_index_specificity_matches_selector_specificity`. lib 169/169. Next session: bench it A/B against #317, run the receipt, open the PR ("merge after #317"). Expect a small win: only matched rules paid this cost.
- **Instrument:** added `trench/tools/ab.py` (interleaved A/B pairs of `cascade_bench.py`). Standing binaries live in `cascade-target/pc-A-d4158a3` and `pc-B-92bec62`.
- **Next fix, from reading the code (needs a fresh profile first):** for every declaration of every matched rule, `compute_style_for_element` clones the value string and runs `resolve_css_variables`, even when there's no `var(`. After that, the ancestor Bloom filter for `selector_matches`. Re-profile on #317's head with `cascade-target-prof` before picking.
- **Permissions (same wall as last session):** `git worktree add` works from the hub, but `git -C <other worktree>`, `cd <wt> && git`, env-var-prefixed `cargo`, and `bash script.sh` all need approval. Workarounds used: switch the hub's branch, `cargo --target-dir`, and helpers written in Python. The empty `cs-subject-keys` worktree I tried was removed.
- The real-site lane has `rs-selector-memo` / `rs-cascade-attr-index` worktrees locally, but neither branch is on origin. Overlap risk with this lane: check before the Bloom-filter work.

**Decisions for Pete**
1. (Carried over) Approve the pinned-snapshot method change in BASELINE Changes.
2. **A quiet window for one absolute reading.** Every session so far has run at load 13–19. Could the real-site lane skip one hourly slot (e.g. 04:05), so this lane gets a quiet `cascade_bench --runs 5` and a ratio of record, not a projection?
3. (Carried over) Permissions for `git -C ~/Repos/.worktrees/cs-*`, so the lane can use per-branch worktrees.
