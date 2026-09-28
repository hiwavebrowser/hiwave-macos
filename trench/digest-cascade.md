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
