# HiWave macOS: catch-up report since V1.4.0

**Repository:** hiwavebrowser/hiwave-macos
**Window:** release V1.4.0 (master `200cb603`, published 2026-10-23 21:07 ET) to develop `05bdd2b2` (2026-11-10 10:05 ET)
**Prepared:** 2026-11-10, ET (America/New_York). Author: Herald (automated reporting).
**Type:** first report; baseline and catch-up. Later reports will cover one period each.

All figures below come from the GitHub API and the git history of this repository unless a section says otherwise. Where a figure was not available, the report says so; nothing is estimated.

## Summary

- **Volume.** develop is 1,281 commits ahead of V1.4.0 (891 non-merge commits) and 0 behind. 227 pull requests were merged into develop. The net diff touches 504 files: +148,360 / −8,300 lines.
- **Main features.** The browser UI (the "chrome") is now drawn by RustKit by default on macOS (#939), replacing the system WebView path, which stays available as a fallback. Pages gained text selection, clipboard and undo in text fields, macOS text input/IME, drag and drop, a RustKit `<select>` popup, canvas 2D (phase 1), CSS transitions, `@keyframes` and Web Animations, `@container` queries, `clip-path`, `filter` and `backdrop-filter`, styled scrollbars and nested scroll containers.
- **Responsiveness.** A series of latency fixes (LAT2 to LAT17) landed: page loads now yield between phases (#1006), scripts run as they arrive (#1013), and runaway scripts are stopped and recovered (#1002). Tab handling gained close and release fixes (#1003, #1004, #1005) and keyboard tab switching (#1010, #1012).
- **Security.** Two P0 fixes landed today: the history-list script-injection bug in the privileged chrome page (#1029, plan row SEC1) and its follow-up, which escapes and type-checks Inspector data (#1033, SEC1b). Autofill hardening also landed (#1034).
- **Measured progress (Null-board figures, relayed by ZeuzGb; repository sources agree, see below).** Chrome board 5/42 → **21/42** green under both rules; WPT tier-1 sample **104/349**; real-site board **26/57** at batch 55. Measured at develop `74a6e47c`. The current tip `05bdd2b2` adds one documentation-only merge (#1046) on top of it.
- **Open risks.** develop failed to compile or pass a core suite for about 8 hours early on 2026-11-10 (issue #1035, now closed). Develop Compile has no completed result yet for the current tip. One open PR (#1042) targets `master` directly. The issues list has no priority labels, so P0 and P1 items can only be tracked in the plan file.

## What shipped

Merged PRs are listed in full, with links, in the appendix. Highlights by area:

**Chrome/UI (the Chrome look-alike work).**
- RustKit chrome: behind a flag (#907), composed over the page (#909, #925, #927), shelf, Inspector and settings as RustKit views (#910, #911), and made the default on macOS (#939). `HIWAVE_RUSTKIT_CHROME=0` keeps the WebKit chrome.
- Chrome board tooling (#898) and its second green rule (#961). Board rounds 1–3 (#997) took the board from 18/42 to 21/42. Later fixes: a stray chevron over the reload button (#1025) and form-control radius, dashes and `appearance: none` (#1027).
- Interaction: selection and `getSelection()` (#895), drag and drop (#896), clipboard, undo/redo and word movement in text fields (#897), select-all and caret (#912), mouse surface (context menu, double-click word select, CSS cursor, tooltips) (#917), macOS text input with IME and dead keys (#918), RustKit select drop-down over the page (#899, #929).
- Tabs and shortcuts: Cmd+W (#1004), tab strip order (#1010), Cmd+1..9 and Ctrl+Tab from a focused page (#1012), page-first Cmd chords with a 100 ms limit (#1015, #1023), Escape in the address bar (#1022), atomic session save (#1018).
- Developer tools: Inspector event log, script lifecycle, network facts, `diagnose` cause report and a service thread for the console (#792, #797, #802, #803, #809, #810, #835, #838, #839).

**Engine, layout, CSS and DOM.**
- CSS: `@container` size queries (#782), adopted stylesheets (#788, #816), `display: contents` and popover boxes (#786), complex selectors in `:is()/:where()/:not()` (#822, #890), CSS Color 4 syntax (#870), outline (#867), 3D transform parsing (#853), `<style media>` (#964), attribute case flag (#968).
- Layout: sticky positioning (#851, #868), nested scroll containers and wheel routing (#845, #846, #850), flex and grid sizing fixes (#868, #869, #871, #883, #888, #902, #922), abspos static positions (#830, #976, #983), `getComputedStyle` covering about 150 properties (#866).
- DOM and scripting: `Element.animate()` (#881), Window named access (#887), `Range` mutation methods (#1024), `innerText` driven by computed style (#923, #951), one fetch per module URL and whole import-graph prefetch (#882, #891).

**Paint and rendering.** CSS `fill`/`stroke` on inline SVG (#787), `clip-path` basic shapes, `rect()`/`xywh()` and reference boxes (#789, #970, #971, #981, #982), `filter` (#790), `backdrop-filter` (#930), styled scrollbars (#900), canvas 2D phase 1 (#905), CSS transitions and `@keyframes` (#884, #885, #889), one encoder and one vertex buffer per frame (#992; 50 layers 10.3 → 1.35 ms/frame per the plan file).

**Latency and performance.** Script geometry reads force layout only when the tree is dirty (#812, #947). Paint-only animation frames skip relayout (#941). Prepared stylesheets are kept across relayouts (#942). `:has()` invalidation is scoped (#945). Animation layers reset per document (#1000). Input-to-present is measured (#1001). Page loads yield between phases (#1006), scripts run as they arrive (#1013), and each image gets its own download budget (#1016). Per the plan file, LAT15 cut the longest turn of a slow load from 12.2 s to 2.1 s.

**Security.** App bridge only for app-owned documents (#920), history-list escaping (#1029, P0), Inspector data escaping and type checks (#1033, P0), autofill hardening (#1034). Page URLs in logs now drop query strings (#932, #946).

**CI and tests.** WPT reftest runner with "proven" passes (#844, #856, #857), CSS property census (#842, #860, #873), portable test pins (#859, #872, #879, #958, #980, #996, #1014), superseded PR runs cancelled (#1021), macOS jobs moved off the retired macos-14 image (#1028), and Parity Gate made green again (#1032).

## Progress in numbers

### hiwave-macos (V1.4.0 → develop `05bdd2b2`)

| Measure | Value | Source |
|---|---:|---|
| Commits on develop not on V1.4.0 | 1,281 (891 non-merge, 390 merge) | git history |
| Merged PRs into develop | 227 | GitHub search, merged after 2026-10-23 21:07 ET |
| Merged PRs by week (ET) | Oct 24–25: 1 · Oct 26–Nov 1: 67 · Nov 2–8: 128 · Nov 9–10: 31 | GitHub search |
| Lines added / removed (net diff) | +148,360 / −8,300 | `git diff --shortstat V1.4.0 develop` |
| Files touched (net diff) | 504 (228 with "test" in the path) | git diff |
| Of which under `crates/` | 348 files, +101,521 / −7,811 | git diff |
| Largest areas by lines added | rustkit-engine +56,786; docs +20,146; rustkit-bindings +9,860; hiwave-app +8,853; parity oracle tools +7,421; rustkit-layout +6,252 | git diff numstat |
| Issues closed in window | 2 (#965, #1035) | GitHub search |
| Issues open now | 28 (none carries a priority or regression label) | GitHub search |
| Open PRs now | 12 (10 drafts; 11 target develop, 1 targets master) | GitHub API |
| Tags/releases in window | No new release. 12 `parked/2026-10-20/*` archive tags exist (dated before the window); no tag points at a commit after V1.4.0. | GitHub API |

**Merged PRs by area** (classified by title; each PR is counted once):

| Area | PRs |
|---|---:|
| Engine, layout, CSS and DOM | 76 |
| Chrome/UI and developer tools | 44 |
| CI, tests and tooling | 41 |
| Paint, rendering and animation | 33 |
| Latency and performance | 27 |
| Security | 4 |
| Other (documentation) | 2 |
| **Total** | **227** |

**Contributors.**
- PR authors: `petec4244` (217 PRs) and `cursor[bot]` (10 PRs).
- Commit identities in the range: the owner's identities (1,174 commits), Atlas (39), Claude (46), Cursor Agent (22).
- `Co-Authored-By` trailers name Claude models on 908 commits. 572 commits carry a `Claude-Session` trailer.
- Seats: commit authorship identifies only Atlas. PR text names Prometheus as the usual first reviewer (R1) and Athena, Talos, Pollux and Castor as reviewers or port leads. Seat attribution for most commits cannot be determined from the repository.

**CI on develop** (workflow runs on pushes to `develop` since 2026-10-24):

| Workflow | Runs | Success | Failure | Cancelled | Queued/in progress | Pass rate (completed, excl. cancelled) |
|---|---:|---:|---:|---:|---:|---:|
| Develop Compile | 50 | 30 | 8 | 7 | 5 | 30/38 = **78.9 %** |
| Baseline Audit (PR-triggered on develop) | 1 | 1 | 0 | 0 | 0 | 1/1 |
| Parity Gate (PR-triggered on develop) | 1 | 1 | 0 | 0 | 0 | 1/1 |

Parity Gate across all PRs since 2026-10-24: 66 successful and 30 failed runs (68.8 % of runs that concluded success or failure). According to #1032, no Parity Gate runs took place between 2026-10-29 and 2026-11-10. The five queued Develop Compile runs include those for `74a6e47c` and the current tip `05bdd2b2`, so neither had a CI result when this report was prepared.

### Boards (Null-board figures, relayed by ZeuzGb)

| Board | Earlier | Now | Measured at | Repository evidence |
|---|---|---|---|---|
| Chrome board (42 rows, both rules) | 5/42 at the first triage | **21/42** | develop `74a6e47c` | Agrees. `trench/chrome/BOARD.md` records 5/42 on 2026-11-05 (develop `209f6a64`). #997 records 18/42 → 21/42 under both rules. #1025/#1027 record 21/42 at batch 53. |
| WPT tier-1 sample | n/a | **104/349** | develop `74a6e47c` | Agrees. #997 records 104 PASS of 349 on both arms. The plan file records 82/349 on 11-06 and 104/349 from batch 39 onward. |
| Real-site board | n/a | **26/57** at batch 55 | develop `74a6e47c` | Not contradicted, not directly confirmed. The plan file (row RS1) records batch 54 at develop 25/57 and the candidate at 27/57 (19 sites, shopify excluded). V1.4.0 published 28/60 on a different basis (20 sites, 15 % gate), so the two are not comparable. |

The 26-case campaign stays at 26/26 (mean about 1.03 % per the plan file's batch notes). The board's oracle is Chromium 143. Moving it to Chrome for Testing 148 is plan row T43.

## Errors and regressions

**Red CI on develop.**
- 2026-11-10, about 00:38–04:34 ET: seven consecutive Develop Compile runs failed (`e8447bc3` through `d36a8960`). The workflow opened issue [#1035](https://github.com/hiwavebrowser/hiwave-macos/issues/1035) "develop does not compile" (label `develop-broken`) and closed it at 09:18 ET when `13449ff6` (merge of #1032) compiled again. The failing step's own output was not captured in the retrieved log, so the exact cause is not stated here.
- 2026-10-28 16:43 ET: one Develop Compile failure (`db671788`, merge of #821).
- Currently: Develop Compile for `74a6e47c` and `05bdd2b2` is queued, with no result yet. The `74a6e47c` commit has no check runs.

**Known flakes and timing tests.**
- *wgate timing pins:* two LAT15 accounting tests in `rustkit-engine/src/wgate.rs` failed on slow shared runners. #1032 rewrote them to measure stretches directly, without deleting or widening any test.
- *script-guards:* two guards went stale while Parity Gate was not running. #1032 refreshed them.
- Other flakes handled in the window: the console_log label map (#831), real-window profile seeding (#928), close-tab test selection (#1009), and the i0a resize/nav driver (#1008).
- Still open: `i0a_close` Cmd+T/Cmd+W latency check under load (plan row T45). CPU-adapter timing tests [#998](https://github.com/hiwavebrowser/hiwave-macos/issues/998) and [#999](https://github.com/hiwavebrowser/hiwave-macos/issues/999). macOS-specific chrome-board font tests failing on Windows [#1007](https://github.com/hiwavebrowser/hiwave-macos/issues/1007).

**Regressions and reverts.**
- No merged PR was reverted. Two in-branch revert commits landed inside larger PRs: `f19ff1f2` took sticky table parts out of #868, and `921526e7` restored inline content area and select height.
- [#247](https://github.com/hiwavebrowser/hiwave-macos/issues/247) is still open: a WPT regression from kerning (#230), carried since V1.1.0.
- The plan file records #984 (stylesheet links inserted by script) as held because it caused a header-link regression on a major site, bisected to the PR. It is still an open draft.

**Open P0 and P1 items.**
- No open issue carries a P0 or P1 label; the repository's issues are unlabelled. Priorities are tracked only in the plan file.
- P0 items in the plan: SEC1 is done (#1029, #1033 landed today), and autofill hardening (#1034) landed. Follow-up plan rows remain open.
- P1 items in the plan:
  - JS1: the 10M-iterations-per-frame script loop limit can stop legitimate JavaScript in the shipped build after about 50–70 ms.
  - E54: `<img>` does not fire `load` or report `complete`/`naturalWidth`. Draft fix in #1019.
  - CB1: chrome board P1s. Two are fixed by #1025 and #1027; text anti-aliasing is next, in draft #1030.

**Process note.** Open PR [#1042](https://github.com/hiwavebrowser/hiwave-macos/pull/1042), from an automated agent branch, targets `master` rather than `develop`.

## Plan

Sources: the plan file `trench/z/PLAN-z.md` and the week plan `trench/z/WEEK-2026-11-09.md`, both on branch `atlas/z` of hiwavebrowser/hiwave. Neither is in hiwave-macos (see Data gaps). Current direction, per the plan file's 2026-11-09 entry: the focus is making the browser look like Chrome, with the chrome board as the metric; SEC1 lands first (now done); tao removal is parked.

### Short term (next days)
- **Batch 56** (gating): #1038 and #1044 (engine test suite 3.4× and 2.1× faster), #1031 (microtask cap and error timing), #1030 (macOS glyph masks as Chrome draws them), #1041 (chrome IPC logs record shape only), #1019 (E54 `<img>` load), #1040 (structural pseudo-classes on ancestors, E51), #1043 (flex item aspect-ratio). **Batch 57:** #1037 (button heights, CB3/CB3b) and #1039 (picture/srcset selection).
- **Chrome board, 21 red rows:**
  - CB1 remaining P2s: text weight and dimness, inspector indent, square corners on selects.
  - CB2: canvases sized at load read 0×0 (report page).
  - CB3c: button width and max-content.
  - CB4: abspos label and translate (inspector).
  - CB5: reload button turns into Stop while loading.
  - T43: move the board oracle to Chrome for Testing 148.
- **Scripting:** JS1 (loop limit in release builds).
- **Housekeeping:** DOC1 (README still describes the WebView UI) and DEAD1 (remove dead chrome code).

### Medium term (next weeks)
- **CSS week (2026-11-11 to 11-16).** The WPT tier-1 sample becomes the target instead of a no-regression gate. Order: masks and clip-path first (41 cases), then sticky, the abspos bucket (B15 decision), SVG transform attributes, and overflow-clip. The census re-rank also lists SVG CSS paint, `appearance: none`, min/max-content sizing, `display: contents`, `@container`, list markers, `filter` and 3D transforms.
- **Usability milestone** (a normal multi-tab session that stays responsive):
  - ARCH1: macOS tabs share one content view, so a background tab's load is cancelled.
  - M2b: memory grows about 1.5–3 MB per tab cycle.
  - LAT18: animation frames dominate the content thread on heavy pages.
  - LAT17 follow-ups.
- **Platform convergence:** CI parity for hiwave-windows and hiwave-linux (#1020), plus the shared-engine fixes found by the ports.
- **Z plan end date:** 2026-11-20 (from the plan file).

### Long term (next months)
- **"Nothing borrowed" on macOS (NB1):** retire wry, then muda, then tao. Parked; it depends on ARCH1 (per-tab content views).
- **Accessibility tree for the RustKit chrome (A8):** this gates deleting the WebKit chrome and wry (A5b, CH5).
- **Engine architecture:** incremental restyle and partial layout (LAT11) and the absolutely-positioned containing block (B15).
- **JavaScript performance:** the plan notes the Boa engine is about 18× slower than V8 without a JIT, and some sites spend 4–9 s in script per load.
- **Return to live sites (CH6)**, with work re-ranked by what the chrome work taught.

## Data gaps

- **Plan file location:** `PLAN-z.md` is not in hiwave-macos (not on develop, master, or any branch in this repository). The plan sections come from hiwavebrowser/hiwave, branch `atlas/z`. This report does not reproduce that file's internal notes.
- **Real-site board:** the relayed 26/57 at batch 55 has no matching record in either repository. The closest record is batch 54 (25/57 develop, 27/57 candidate).
- **Develop tip CI:** no completed CI result for `74a6e47c` or `05bdd2b2` at the time of writing. Five Develop Compile runs were queued.
- **Cause of the 2026-11-10 develop break:** not extracted. The retrieved log tail shows only the failure exit and the issue-filing step.
- **Line counts per PR and per area:** not computed. Line totals are for the net diff V1.4.0 → develop only. Per-commit totals were not gathered.
- **Priority labels:** issues carry no P0/P1/regression labels, so open P0/P1 counts from the issue tracker are zero by construction. Priorities come from the plan file instead.
- **Area classification:** done from PR titles and checked by hand. Some PRs span several areas.
- **Seat attribution:** only the Atlas seat appears as a commit author. Other seats are identifiable only from PR text, so per-seat counts are not given.
- **Windows and Linux:** out of scope for this report (macOS since V1.4.0 only). Their activity is not counted here.
- **Develop Compile coverage:** the workflow ran on 50 of the pushes to develop in the window. Whether it skips some pushes by design was not checked.

## Appendix: merged PRs since V1.4.0, by area

### Chrome/UI and developer tools (44)

- [#791](https://github.com/hiwavebrowser/hiwave-macos/pull/791) fix: console mirror no longer puts page text in the Inspector (devtools T1b)
- [#797](https://github.com/hiwavebrowser/hiwave-macos/pull/797) feat(devtools): script lifecycle per tab in the event log (T3)
- [#802](https://github.com/hiwavebrowser/hiwave-macos/pull/802) feat(devtools): diagnose — one-page ranked cause report for a URL (T4)
- [#806](https://github.com/hiwavebrowser/hiwave-macos/pull/806) devtools: review follow-ups (#791/#797/#799/#802), unreachable unparseable-inserted branch removed
- [#809](https://github.com/hiwavebrowser/hiwave-macos/pull/809) devtools T6c: busy state + deadlines for live queries; console read during a same-thread load
- [#810](https://github.com/hiwavebrowser/hiwave-macos/pull/810) devtools T6d: console calls reach a listening Inspector as they are made; hot-loop cap with a dropped count
- [#814](https://github.com/hiwavebrowser/hiwave-macos/pull/814) fix(devtools): T6 follow-ups (#809/#810 leftovers)
- [#835](https://github.com/hiwavebrowser/hiwave-macos/pull/835) feat: diagnose says what the frame said (T10: T9 gaps 1, 2, 4, 5)
- [#838](https://github.com/hiwavebrowser/hiwave-macos/pull/838) feat: diagnose names overlap, overflow and own-site string errors (T11)
- [#895](https://github.com/hiwavebrowser/hiwave-macos/pull/895) feat(engine): selection model, mouse selection, painted highlight, real getSelection() (CH2.E2)
- [#896](https://github.com/hiwavebrowser/hiwave-macos/pull/896) feat(engine): HTML5 drag and drop events with DataTransfer (CH2.E5)
- [#897](https://github.com/hiwavebrowser/hiwave-macos/pull/897) feat(forms): clipboard, cut/copy/paste, undo/redo and word movement in text controls (CH2.E1)
- [#899](https://github.com/hiwavebrowser/hiwave-macos/pull/899) feat(engine): RustKit-drawn `<select>` popup with keyboard and mouse (CH2.E6)
- [#901](https://github.com/hiwavebrowser/hiwave-macos/pull/901) CH2.E9: tell the macOS chrome the committed URL/title/load state; window.ipc pushes to the window
- [#907](https://github.com/hiwavebrowser/hiwave-macos/pull/907) CH3.A2: HIWAVE_RUSTKIT_CHROME=1 chrome in RustKit with its input routed; typed URL navigates (3a+3b+3c)
- [#909](https://github.com/hiwavebrowser/hiwave-macos/pull/909) CH3.A3: RustKit chrome composed over the page (pass-through, focus, resize) — stacked on #907
- [#910](https://github.com/hiwavebrowser/hiwave-macos/pull/910) CH3.A4: shelf, Inspector and settings as RustKit views (flag on), stacked on #909
- [#911](https://github.com/hiwavebrowser/hiwave-macos/pull/911) fix(app): #907 R1 nits + CH3.A4b lazy panels (stacked on #910)
- [#912](https://github.com/hiwavebrowser/hiwave-macos/pull/912) CH2.E12: select-all / select() / setSelectionRange / click caret in text controls
- [#914](https://github.com/hiwavebrowser/hiwave-macos/pull/914) fix(app): Save Settings writes settings.json and the page shows the outcome (CH2.B11)
- [#917](https://github.com/hiwavebrowser/hiwave-macos/pull/917) feat: CH2.E4 mouse surface (contextmenu, dblclick/word select, CSS cursor, tooltips)
- [#918](https://github.com/hiwavebrowser/hiwave-macos/pull/918) feat(viewhost,engine): macOS text input (NSTextInputClient, IME, dead keys), Edit-menu responders, Cmd chords to the page first (CH2.E3)
- [#919](https://github.com/hiwavebrowser/hiwave-macos/pull/919) fix(viewhost): macos_input_order test handles PointerInput::Secondary (develop compile fix after #917)
- [#927](https://github.com/hiwavebrowser/hiwave-macos/pull/927) CH3.A3c: the RustKit chrome as a transparent surface over the page (alpha route), stacked on #925
- [#929](https://github.com/hiwavebrowser/hiwave-macos/pull/929) CH2.E6b: the RustKit chrome's select drop-down over the page: modal while open, select background (stacked on #927)
- [#932](https://github.com/hiwavebrowser/hiwave-macos/pull/932) fix(app,shell,engine): page URLs in the info log are scheme, host and path (CH3.A6)
- [#939](https://github.com/hiwavebrowser/hiwave-macos/pull/939) feat(app): RustKit chrome is the default on macOS; HIWAVE_RUSTKIT_CHROME=0 keeps the WebKit chrome (CH3.A5a)
- [#946](https://github.com/hiwavebrowser/hiwave-macos/pull/946) fix: resource and page URLs on info/warn lines drop the query (CH3.A6b)
- [#948](https://github.com/hiwavebrowser/hiwave-macos/pull/948) fix: CH3.A9 chrome commands no longer wait out the content thread's idle sleep (Cmd+T)
- [#955](https://github.com/hiwavebrowser/hiwave-macos/pull/955) CH3.A10: chrome state is one script per turn, applied at most 5x/s (chrome relayouts while loading)
- [#997](https://github.com/hiwavebrowser/hiwave-macos/pull/997) chrome board rounds 1–3: emoji fallback, heading em sizes, attribute escapes, inline half-leading, scorer fixes (CH2.B31–B37)
- [#1003](https://github.com/hiwavebrowser/hiwave-macos/pull/1003) fix(viewhost): closing a tab releases its view and surface (E35)
- [#1004](https://github.com/hiwavebrowser/hiwave-macos/pull/1004) fix(chrome): Cmd+W closes the active tab every time (M1)
- [#1005](https://github.com/hiwavebrowser/hiwave-macos/pull/1005) fix(engine): navigation releases the previous document (M2)
- [#1010](https://github.com/hiwavebrowser/hiwave-macos/pull/1010) fix(shell): tabs in one explicit strip order; Cmd+N picks the Nth opened tab (M1c)
- [#1011](https://github.com/hiwavebrowser/hiwave-macos/pull/1011) fix(engine): querySelector honours attribute/:not filters on ancestor compounds (CH2.E50)
- [#1012](https://github.com/hiwavebrowser/hiwave-macos/pull/1012) fix(app): Cmd+1..9, Cmd+Shift+[ ] and Ctrl+Tab switch tabs from a focused page (M1d; after #1010)
- [#1015](https://github.com/hiwavebrowser/hiwave-macos/pull/1015) fix(viewhost): page-first Cmd chords wait at most 100 ms for the page (M1e; lands after #1012)
- [#1018](https://github.com/hiwavebrowser/hiwave-macos/pull/1018) M4 + M4b item 3: atomic session save, keep an unreadable state file aside (port of hiwave-windows #147)
- [#1022](https://github.com/hiwavebrowser/hiwave-macos/pull/1022) fix(chrome): Escape in the address bar reverts or stops, never closes the sidebar (M1f)
- [#1023](https://github.com/hiwavebrowser/hiwave-macos/pull/1023) fix(viewhost): a timed-out Cmd chord has one actor; forgotten tokens stay timed out (M1g)
- [#1025](https://github.com/hiwavebrowser/hiwave-macos/pull/1025) fix(renderer): clip vector primitives to the clip in force (chrome board CB1a: stray chevron over reload)
- [#1026](https://github.com/hiwavebrowser/hiwave-macos/pull/1026) fix(viewhost): rustkit-viewhost compiles on Windows again (InputEvent imported twice)
- [#1027](https://github.com/hiwavebrowser/hiwave-macos/pull/1027) fix(layout): form controls keep author radius/dashes; carry appearance:none (chrome board CB1b)

### Engine, layout, CSS and DOM (76)

- [#777](https://github.com/hiwavebrowser/hiwave-macos/pull/777) fix: innerHTML on script, style, textarea and title makes one text node (HTML §13.4)
- [#782](https://github.com/hiwavebrowser/hiwave-macos/pull/782) rustkit: apply @container size queries instead of dropping them
- [#786](https://github.com/hiwavebrowser/hiwave-macos/pull/786) fix: display contents, blockification and popover boxes (re-cut of 243374eb)
- [#788](https://github.com/hiwavebrowser/hiwave-macos/pull/788) fix: apply adopted stylesheets and keep Lit template markers (re-cut of e1d795a4)
- [#799](https://github.com/hiwavebrowser/hiwave-macos/pull/799) fix: load_html runs the document's inline scripts (T8)
- [#804](https://github.com/hiwavebrowser/hiwave-macos/pull/804) fix: a Document takes a new root after the old one is removed (DOM §4.2.3)
- [#807](https://github.com/hiwavebrowser/hiwave-macos/pull/807) fix: Element.removeAttributeNode and Intl.Locale (yahoo blank after React hydration recovery)
- [#811](https://github.com/hiwavebrowser/hiwave-macos/pull/811) fix(css): #782 follow-up nits (container name split, one hover walk, quoted parens)
- [#816](https://github.com/hiwavebrowser/hiwave-macos/pull/816) fix: #788 follow-ups - observable adoptedStyleSheets, disabled/media republish, O(n) adopters
- [#817](https://github.com/hiwavebrowser/hiwave-macos/pull/817) fix: display: inherit copies the parent's display
- [#818](https://github.com/hiwavebrowser/hiwave-macos/pull/818) fix: window.matchMedia evaluates queries against the viewport (HT27 N2)
- [#819](https://github.com/hiwavebrowser/hiwave-macos/pull/819) fix(bindings): on<type> handlers are accessors on HTMLElement.prototype (yahoo onerror throw)
- [#821](https://github.com/hiwavebrowser/hiwave-macos/pull/821) fix: getComputedStyle(el).content reports the cascaded string (HT27 N2)
- [#822](https://github.com/hiwavebrowser/hiwave-macos/pull/822) fix: complex selectors inside :is()/:where()/:not() match (HT27 R1)
- [#823](https://github.com/hiwavebrowser/hiwave-macos/pull/823) fix: positioned <img> boxes take their positioning and re-resolve percent sizes on re-anchor (HT27 R3)
- [#825](https://github.com/hiwavebrowser/hiwave-macos/pull/825) fix(bindings): body/frameset Window event handlers forward to the Window (HTML §8.1.8.2, BW1)
- [#828](https://github.com/hiwavebrowser/hiwave-macos/pull/828) fix: :is() compound members beside a complex one read attributes and :not() (CA1-1)
- [#829](https://github.com/hiwavebrowser/hiwave-macos/pull/829) fix: a ::before/::after with display: none generates no box (RD2)
- [#830](https://github.com/hiwavebrowser/hiwave-macos/pull/830) fix: abspos flex children take their static position unsafely and in the final box (netflix hero, HT27F N1)
- [#837](https://github.com/hiwavebrowser/hiwave-macos/pull/837) fix: getComputedStyle reads a pseudo-element's style
- [#840](https://github.com/hiwavebrowser/hiwave-macos/pull/840) C3: computed display keywords, boxless style reads, z-index 0, custom properties
- [#841](https://github.com/hiwavebrowser/hiwave-macos/pull/841) fix: the flex shorthand takes its keywords and a basis written first
- [#843](https://github.com/hiwavebrowser/hiwave-macos/pull/843) fix(engine): position: relative offsets move the box (Z C13)
- [#846](https://github.com/hiwavebrowser/hiwave-macos/pull/846) feat(scroll): a wheel moves the box under the pointer (C2b, on #845)
- [#850](https://github.com/hiwavebrowser/hiwave-macos/pull/850) fix(scroll): scrollIntoView and fragments move every enclosing scroller (C2c)
- [#851](https://github.com/hiwavebrowser/hiwave-macos/pull/851) fix(sticky): position sticky sticks to its nearest scroller or the window (C2d)
- [#853](https://github.com/hiwavebrowser/hiwave-macos/pull/853) fix(transform): parse the 3D functions, flattened; a bad value refuses the declaration (C10b2)
- [#854](https://github.com/hiwavebrowser/hiwave-macos/pull/854) fix(geometry): a fixed box reads where it is, wherever the window has scrolled
- [#855](https://github.com/hiwavebrowser/hiwave-macos/pull/855) fix(transform): translate takes a function with spaces in it, and flattens a depth
- [#862](https://github.com/hiwavebrowser/hiwave-macos/pull/862) fix(css): a depth refuses a percentage in calc; negative perspective in any unit (C10, batch trial)
- [#863](https://github.com/hiwavebrowser/hiwave-macos/pull/863) fix(layout): floats and empty blocks keep their margins straight (C10a, batch trial)
- [#864](https://github.com/hiwavebrowser/hiwave-macos/pull/864) fix(cssom): offsetParent stops at a table cell or table (C10c, batch trial)
- [#865](https://github.com/hiwavebrowser/hiwave-macos/pull/865) fix(layout): details shows its summary first, and a default one (C10d, batch trial)
- [#866](https://github.com/hiwavebrowser/hiwave-macos/pull/866) fix(cssom): getComputedStyle reads ~150 parsed properties (C9 item 0)
- [#868](https://github.com/hiwavebrowser/hiwave-macos/pull/868) fix(flex): sticky in flex and grid; stretched items take the line's height (C7)
- [#869](https://github.com/hiwavebrowser/hiwave-macos/pull/869) fix(flex): aspect-ratio through the flexed size for non-replaced items (C10e)
- [#870](https://github.com/hiwavebrowser/hiwave-macos/pull/870) fix(css): rgb()/hsl() read CSS Color 4 space syntax (CSS week)
- [#871](https://github.com/hiwavebrowser/hiwave-macos/pull/871) fix(grid): replaced items in grid take replaced sizing and start alignment (C10e, replaced slice)
- [#875](https://github.com/hiwavebrowser/hiwave-macos/pull/875) fix: a redirected navigation commits the last hop's URL (S1, gmail unstyled)
- [#877](https://github.com/hiwavebrowser/hiwave-macos/pull/877) fix: a stylesheet that misses the load's budget applies when it arrives (S6)
- [#878](https://github.com/hiwavebrowser/hiwave-macos/pull/878) fix: sibling combinators above the subject read that ancestor's siblings (S7, eBay checkbox icons)
- [#882](https://github.com/hiwavebrowser/hiwave-macos/pull/882) fix(engine): one fetch per module URL across the page's module tags (S12 microsoft)
- [#883](https://github.com/hiwavebrowser/hiwave-macos/pull/883) fix(flex): column items stretch to the container's own width (S3 gnews grid overlap)
- [#888](https://github.com/hiwavebrowser/hiwave-macos/pull/888) fix(grid): spanning items do not grow minmax(0, 1fr) tracks (S17, gnews card width)
- [#890](https://github.com/hiwavebrowser/hiwave-macos/pull/890) fix(css): complex selectors inside :is()/:where()/:not() match (R1, Tailwind dark:)
- [#892](https://github.com/hiwavebrowser/hiwave-macos/pull/892) fix(engine): input to a document being left is not dispatched or queued ahead of its navigation; the 959 budget stops were the navigation's interrupt word (S2, re-scoped)
- [#893](https://github.com/hiwavebrowser/hiwave-macos/pull/893) fix(html): a second `<body>`/`<html>` start tag merges attributes instead of nesting (S11, gmail root margin)
- [#902](https://github.com/hiwavebrowser/hiwave-macos/pull/902) fix(flex): a row's resolved percentage height is its definite cross size (CH2.B1)
- [#903](https://github.com/hiwavebrowser/hiwave-macos/pull/903) fix(layout): a zero-width overflow:hidden box clips its content (CH2.B2, collapsed chrome sidebar)
- [#913](https://github.com/hiwavebrowser/hiwave-macos/pull/913) fix(engine,layout): height:0 / max-height:0 read as auto; max-height column flex; min over max (CH2.B7)
- [#915](https://github.com/hiwavebrowser/hiwave-macos/pull/915) fix(engine): explicit html height is the root rect; percent chain pins (CH2.B8)
- [#916](https://github.com/hiwavebrowser/hiwave-macos/pull/916) fix(engine): a display:none child is no flex or grid item (CH2.B10)
- [#921](https://github.com/hiwavebrowser/hiwave-macos/pull/921) fix(css): a bare 0 parses to Px(0.0); Length::Zero is only the unset default (CH2.B14)
- [#922](https://github.com/hiwavebrowser/hiwave-macos/pull/922) fix(layout): nested flex container cross size + unsafe center/end overflow (CH2.B9 items 2 and 3)
- [#923](https://github.com/hiwavebrowser/hiwave-macos/pull/923) fix(bindings): innerText reads computed display and visibility (CH2.E13)
- [#924](https://github.com/hiwavebrowser/hiwave-macos/pull/924) fix(layout): fixed form controls size against the viewport; vh/rem/calc control sizes (CH2.B13)
- [#926](https://github.com/hiwavebrowser/hiwave-macos/pull/926) fix(layout): a float's percentage height resolves against its containing block, not the viewport (CH2.B16)
- [#936](https://github.com/hiwavebrowser/hiwave-macos/pull/936) fix(layout): author width/height on text inputs and textareas sizes the content box (CH2.B17)
- [#937](https://github.com/hiwavebrowser/hiwave-macos/pull/937) fix(css): var() resolves on ::before/::after (CH2.E14)
- [#940](https://github.com/hiwavebrowser/hiwave-macos/pull/940) fix(engine): :hover follows layout and scroll under a still pointer (CH2.E15)
- [#950](https://github.com/hiwavebrowser/hiwave-macos/pull/950) fix(engine): a border side with a style and no width is medium; thin/medium/thick widths (CH2.B18 (1))
- [#951](https://github.com/hiwavebrowser/hiwave-macos/pull/951) fix(bindings): innerText reads white-space, table breaks and a boxless <p> from computed style (CH2.E13b)
- [#956](https://github.com/hiwavebrowser/hiwave-macos/pull/956) fix(css): an element var() with no value and no fallback computes as unset (CH2.E14b)
- [#964](https://github.com/hiwavebrowser/hiwave-macos/pull/964) fix(css): <style media> and <style>.sheet.disabled (CH2.E17)
- [#968](https://github.com/hiwavebrowser/hiwave-macos/pull/968) fix(css): attribute selector case flag [a="v" i] (CH2.E18)
- [#972](https://github.com/hiwavebrowser/hiwave-macos/pull/972) CH2.E27: split a style attribute as a declaration list (data URLs survive)
- [#973](https://github.com/hiwavebrowser/hiwave-macos/pull/973) CH2.E28: an empty declaration no longer eats the next one
- [#976](https://github.com/hiwavebrowser/hiwave-macos/pull/976) fix(layout): an abspos box's static position is past the pending bottom margin (CH2.B24)
- [#977](https://github.com/hiwavebrowser/hiwave-macos/pull/977) fix(bindings,engine): a rejected inline bad url reads the cascaded value (CH2.E29)
- [#983](https://github.com/hiwavebrowser/hiwave-macos/pull/983) fix(layout): abspos static position after a line and in a grid; column flex end margin (CH2.B25)
- [#986](https://github.com/hiwavebrowser/hiwave-macos/pull/986) fix(layout): author border-width on <input> overrides the UA border (CH2.B18c)
- [#987](https://github.com/hiwavebrowser/hiwave-macos/pull/987) fix(css): border shorthand with width only does not set a style (CH2.B18b)
- [#993](https://github.com/hiwavebrowser/hiwave-macos/pull/993) fix(css): UA block margins are em, resolved against the element font size (CH2.B30)
- [#994](https://github.com/hiwavebrowser/hiwave-macos/pull/994) fix(cssparser): braces inside CSS strings do not end blocks (CH2.E31)
- [#1002](https://github.com/hiwavebrowser/hiwave-macos/pull/1002) fix(engine): the script budget stops a runaway script and the page recovers (H19)
- [#1024](https://github.com/hiwavebrowser/hiwave-macos/pull/1024) fix(bindings): Range.surroundContents, insertNode, extractContents (CH2.E47)

### Paint, rendering and animation (33)

- [#787](https://github.com/hiwavebrowser/hiwave-macos/pull/787) rustkit: author CSS fill/stroke/stop-color paint inline svg children
- [#789](https://github.com/hiwavebrowser/hiwave-macos/pull/789) css: clip-path basic shapes (inset/circle/ellipse) clip the box and its descendants
- [#790](https://github.com/hiwavebrowser/hiwave-macos/pull/790) rustkit: CSS filter colour-matrix functions (grayscale/sepia/saturate/hue-rotate/invert/opacity/brightness/contrast)
- [#813](https://github.com/hiwavebrowser/hiwave-macos/pull/813) fix(filter): #790 follow-up nits + filter/clip-path pin
- [#824](https://github.com/hiwavebrowser/hiwave-macos/pull/824) fix: position: relative boxes paint in the positioned layer with their z-index (HT27 R2, railway hero)
- [#826](https://github.com/hiwavebrowser/hiwave-macos/pull/826) fix: a static flex/grid item with a z-index paints positioned (HT27 N1b, netflix hero)
- [#832](https://github.com/hiwavebrowser/hiwave-macos/pull/832) fix: a url() paint that names no server paints its fallback colour (RD4)
- [#833](https://github.com/hiwavebrowser/hiwave-macos/pull/833) fix: paint the frame a keyframe animation settles on (RD3)
- [#836](https://github.com/hiwavebrowser/hiwave-macos/pull/836) fix: parsed SVG elements take the SVG namespace and their real names
- [#845](https://github.com/hiwavebrowser/hiwave-macos/pull/845) feat(scroll): an overflow box scrolls in paint, hit test and rects (C2a)
- [#852](https://github.com/hiwavebrowser/hiwave-macos/pull/852) fix(geometry): rects of transformed boxes read where they paint (C10b)
- [#861](https://github.com/hiwavebrowser/hiwave-macos/pull/861) fix(css): opacity takes a percentage and calc (batch trial)
- [#867](https://github.com/hiwavebrowser/hiwave-macos/pull/867) fix(outline): parse, paint and read the outline family (C5)
- [#874](https://github.com/hiwavebrowser/hiwave-macos/pull/874) fix: visually hidden boxes stop painting: legacy clip rect, clip-path on controls (S5)
- [#880](https://github.com/hiwavebrowser/hiwave-macos/pull/880) fix(layout): clip-path polygon() hides visually-hidden boxes (Z row S5, reopened)
- [#881](https://github.com/hiwavebrowser/hiwave-macos/pull/881) feat: Element.animate() minimal Web Animations (S8, github blank hero)
- [#884](https://github.com/hiwavebrowser/hiwave-macos/pull/884) feat: CSS transitions run on computed-value change (S13a, stacked on #881)
- [#885](https://github.com/hiwavebrowser/hiwave-macos/pull/885) feat: @keyframes animations run (S13b, stacked on #884)
- [#889](https://github.com/hiwavebrowser/hiwave-macos/pull/889) feat: WAAPI animated-style layer, getAnimations() for CSS animations, playbackRate/reverse (S13c, stacked on #885)
- [#900](https://github.com/hiwavebrowser/hiwave-macos/pull/900) feat(paint): styled scrollbars (::-webkit-scrollbar, scrollbar-width/color) on scroll containers (CH2.E7)
- [#904](https://github.com/hiwavebrowser/hiwave-macos/pull/904) fix(engine,layout): transparent buttons paint nothing; button labels collapse whitespace (CH2.B3)
- [#905](https://github.com/hiwavebrowser/hiwave-macos/pull/905) feat: canvas 2D phase 1 — CanvasRenderingContext2D, replaced box, vector paint (CH2.E10)
- [#908](https://github.com/hiwavebrowser/hiwave-macos/pull/908) fix(engine): text-transform applies to the laid-out run, not only paint (CH2.B6)
- [#930](https://github.com/hiwavebrowser/hiwave-macos/pull/930) CH2.E8: backdrop-filter paints the filtered backdrop (blur/saturate/brightness/contrast/opacity)
- [#931](https://github.com/hiwavebrowser/hiwave-macos/pull/931) fix(text): system-font weights set the wght axis, as Chrome does (CH2.B6c)
- [#952](https://github.com/hiwavebrowser/hiwave-macos/pull/952) fix(engine): ::before/::after keyframe animations and transitions run (CH2.E16)
- [#953](https://github.com/hiwavebrowser/hiwave-macos/pull/953) fix(engine): :hover, :active, :focus-within and :checked reach inline svg paint rules (CH2.E19)
- [#954](https://github.com/hiwavebrowser/hiwave-macos/pull/954) CH2.E22: :focus/:focus-within/:focus-visible in the cascade, paint-only focus restyle (stacked on #953)
- [#970](https://github.com/hiwavebrowser/hiwave-macos/pull/970) fix(layout): clip-path rect() and xywh() clip to their rectangle (C8)
- [#971](https://github.com/hiwavebrowser/hiwave-macos/pull/971) fix(css,layout): SVG reference boxes on HTML boxes and a lone <geometry-box> clip-path (C8)
- [#981](https://github.com/hiwavebrowser/hiwave-macos/pull/981) C8d: clip-path inset round <percent> resolves against the reference box
- [#982](https://github.com/hiwavebrowser/hiwave-macos/pull/982) C8d: clip-path on non-replaced inline boxes (first fragment; margin-box vertical margins)
- [#1017](https://github.com/hiwavebrowser/hiwave-macos/pull/1017) fix(renderer): E52 netflix text wiggle - install the view's web fonts before a paint-only frame builds its display list

### Latency and performance (27)

- [#803](https://github.com/hiwavebrowser/hiwave-macos/pull/803) feat(devtools): T6 service thread: console off the page thread, push not poll, tab ring, R1 conditions, T6c start
- [#808](https://github.com/hiwavebrowser/hiwave-macos/pull/808) refactor: style and layout build in an Rc LayoutCtx (forced layout step a, no behaviour change)
- [#812](https://github.com/hiwavebrowser/hiwave-macos/pull/812) feat: forced layout on script geometry reads (FL step b)
- [#815](https://github.com/hiwavebrowser/hiwave-macos/pull/815) feat(engine): T7a live script requests off the page thread (rustkit-net) + devtools network read
- [#827](https://github.com/hiwavebrowser/hiwave-macos/pull/827) feat(net): slot ticket on the page thread, two rustkit-net workers, T7a carry-overs (devtools T8)
- [#839](https://github.com/hiwavebrowser/hiwave-macos/pull/839) feat: the hold line names script turns, parse, style and a load's waits (T12)
- [#876](https://github.com/hiwavebrowser/hiwave-macos/pull/876) fix: coalesce adjacent pointer moves on the content thread (S2; navigation waited 5.5 s behind stale moves)
- [#891](https://github.com/hiwavebrowser/hiwave-macos/pull/891) perf(engine): fetch a module's whole static import graph on discovery (S15, microsoft import waterfalls)
- [#894](https://github.com/hiwavebrowser/hiwave-macos/pull/894) perf(engine): hover changes restyle only the affected subtree; sibling-reach per compound (S22, reddit move lag)
- [#938](https://github.com/hiwavebrowser/hiwave-macos/pull/938) trench(latency): LAT0 baseline from hand-test and batch logs
- [#941](https://github.com/hiwavebrowser/hiwave-macos/pull/941) fix(engine): paint-only animation frames skip the relayout (LAT2)
- [#942](https://github.com/hiwavebrowser/hiwave-macos/pull/942) perf(engine): LAT4 keep prepared stylesheets across relayouts
- [#943](https://github.com/hiwavebrowser/hiwave-macos/pull/943) fix(engine): :hover inside :has() and complex :not() keeps the short restyle path (LAT3)
- [#944](https://github.com/hiwavebrowser/hiwave-macos/pull/944) perf(engine): inline svg <use> resolved through an id index and cached across relayouts (LAT4b)
- [#945](https://github.com/hiwavebrowser/hiwave-macos/pull/945) fix(engine): scope :has() invalidation to the subjects above a write (LAT6)
- [#947](https://github.com/hiwavebrowser/hiwave-macos/pull/947) perf(engine): geometry reads on a clean tree don't force a layout; no-op writes aren't writes; forced-layout cause logged (LAT7)
- [#949](https://github.com/hiwavebrowser/hiwave-macos/pull/949) fix: CH3.A4c the Inspector's console appends new lines once a frame (no forced layout per line)
- [#957](https://github.com/hiwavebrowser/hiwave-macos/pull/957) LAT8: snapshot stage - format each element's computed style on its first read; unchanged animation frames publish nothing
- [#969](https://github.com/hiwavebrowser/hiwave-macos/pull/969) perf(engine): LAT7c a Web Animations paint-property frame leaves offset/client reads clean
- [#974](https://github.com/hiwavebrowser/hiwave-macos/pull/974) LAT12: one text-shaping memo for every platform (was macOS-only)
- [#978](https://github.com/hiwavebrowser/hiwave-macos/pull/978) LAT1b: hold summary per content visit, tagged view/host; parser n/a without content entries
- [#992](https://github.com/hiwavebrowser/hiwave-macos/pull/992) perf(renderer): one encoder, one vertex buffer and shared layer textures per frame (LAT13)
- [#1000](https://github.com/hiwavebrowser/hiwave-macos/pull/1000) perf(engine): reset the animation layer per document (LAT14)
- [#1001](https://github.com/hiwavebrowser/hiwave-macos/pull/1001) feat(latency): input-to-present metric (LAT1c)
- [#1006](https://github.com/hiwavebrowser/hiwave-macos/pull/1006) perf(engine): a page load yields between phases (LAT15)
- [#1013](https://github.com/hiwavebrowser/hiwave-macos/pull/1013) LAT17: run page scripts as they arrive, in document order (i0a_nav)
- [#1016](https://github.com/hiwavebrowser/hiwave-macos/pull/1016) LAT9: per-image download budget; keep the old image during a src swap

### Security (4)

- [#920](https://github.com/hiwavebrowser/hiwave-macos/pull/920) CH3.A7: app bridge only for app-owned documents (window.ipc/hiwaveChrome gone from web content)
- [#1029](https://github.com/hiwavebrowser/hiwave-macos/pull/1029) fix(chrome): history list attribute-escaping bug (P0)
- [#1033](https://github.com/hiwavebrowser/hiwave-macos/pull/1033) fix(inspector): escape and type-check page-posted inspector data (SEC1b, P0)
- [#1034](https://github.com/hiwavebrowser/hiwave-macos/pull/1034) fix(autofill): harden credential delivery (SEC2)

### CI, tests and tooling (41)

- [#792](https://github.com/hiwavebrowser/hiwave-macos/pull/792) feat: network facts in the event log: block list/rule, parity-capture --shield, initiator kinds (devtools T2, draft)
- [#796](https://github.com/hiwavebrowser/hiwave-macos/pull/796) fix(parity-capture): the live loop wakes when an inserted script's fetch arrives
- [#798](https://github.com/hiwavebrowser/hiwave-macos/pull/798) test: clean Mac gates (six macOS-only engine tests that failed on every Mac run)
- [#801](https://github.com/hiwavebrowser/hiwave-macos/pull/801) test+ci: empty GET form submit, HTTP 205 abort, doctype and clip-path pins; capture-suites CI (F2c)
- [#820](https://github.com/hiwavebrowser/hiwave-macos/pull/820) test: pin Stop cancelled/stopped paths, FormValue rollup, decode panic
- [#831](https://github.com/hiwavebrowser/hiwave-macos/pull/831) fix: console_log label map is per test thread (CA1-5, develop flake)
- [#834](https://github.com/hiwavebrowser/hiwave-macos/pull/834) test: pin settle opacity, stroke url() fallback, author stroke cascade
- [#842](https://github.com/hiwavebrowser/hiwave-macos/pull/842) feat: CSS property census (dev-tools T13)
- [#844](https://github.com/hiwavebrowser/hiwave-macos/pull/844) T14: WPT reftest runner W0b, first honest count on develop
- [#848](https://github.com/hiwavebrowser/hiwave-macos/pull/848) fix(census): cap the control's credit; read guard and multi-line arms (T13b)
- [#849](https://github.com/hiwavebrowser/hiwave-macos/pull/849) fix(wpt): unquoted attributes in the resource gate; per-case diff reasons (T14b)
- [#856](https://github.com/hiwavebrowser/hiwave-macos/pull/856) test(wpt): coverage for the C-row queue, and a match must go red when broken (T15)
- [#857](https://github.com/hiwavebrowser/hiwave-macos/pull/857) feat: T15b: make WPT "proven" honest (area probes, zero-ink gate, Chromium recheck)
- [#858](https://github.com/hiwavebrowser/hiwave-macos/pull/858) chore: T15c WPT runner, the #857 R1 nits
- [#859](https://github.com/hiwavebrowser/hiwave-macos/pull/859) test: portable pins for transform parse, translate, scroll extents
- [#860](https://github.com/hiwavebrowser/hiwave-macos/pull/860) tools: census per-property prevalence, control notes, guard refusal; WPT area-probe split (T13c)
- [#872](https://github.com/hiwavebrowser/hiwave-macos/pull/872) test: portable pins for opacity, outline, color-keep, aspect-ratio, depth
- [#873](https://github.com/hiwavebrowser/hiwave-macos/pull/873) T13d: census crawl drops a refused-host site on every path; known list pinned to Chromium; one test command
- [#879](https://github.com/hiwavebrowser/hiwave-macos/pull/879) test: portable pins for redirect URL, rem CSSOM, late sheets, clip, collapse
- [#887](https://github.com/hiwavebrowser/hiwave-macos/pull/887) feat(bindings): named access on the Window object (C16, 9 WPT script errors)
- [#898](https://github.com/hiwavebrowser/hiwave-macos/pull/898) tooling: chrome board — HiWave's own UI pages in RustKit vs Chromium (CH3.A1)
- [#906](https://github.com/hiwavebrowser/hiwave-macos/pull/906) test(engine): geometry reads during load already match Chromium — pins + re-scope (CH2.E11)
- [#925](https://github.com/hiwavebrowser/hiwave-macos/pull/925) CH3.A3b: chrome region from its own frames, atomic with the present; HID-safe real-window driver
- [#928](https://github.com/hiwavebrowser/hiwave-macos/pull/928) fix(real-window): every check starts on the seeded profile (CH3.A4b a4_private/e4_chrome_tab flake)
- [#933](https://github.com/hiwavebrowser/hiwave-macos/pull/933) chore: drop scratch/ committed by the batch-24 landing merge
- [#934](https://github.com/hiwavebrowser/hiwave-macos/pull/934) chore: ignore /scratch/ in checkouts
- [#958](https://github.com/hiwavebrowser/hiwave-macos/pull/958) test: portable pins for LAT2 dirty gate, LAT4b svg parts, innerText styles, B17 extras
- [#960](https://github.com/hiwavebrowser/hiwave-macos/pull/960) feat(realsite): T19 oracle_blocked + headed oracle (A1) + n/scorable (A2)
- [#961](https://github.com/hiwavebrowser/hiwave-macos/pull/961) tooling(gates): chrome board second rule (0 geometry + ≤1.5 %), real-site LOOKS RIGHT 20 %, docs/GATES.md
- [#966](https://github.com/hiwavebrowser/hiwave-macos/pull/966) fix(parity-capture): load a fixture's local image files in html-file captures (C8: +14 WPT mask passes)
- [#967](https://github.com/hiwavebrowser/hiwave-macos/pull/967) tooling: LAT1 per-visit hold summary, extract_ms on the relayout line, exit metric in the baseline tool
- [#975](https://github.com/hiwavebrowser/hiwave-macos/pull/975) test(renderer): the buffer-limit test draws a small frame on a CPU adapter (T30)
- [#979](https://github.com/hiwavebrowser/hiwave-macos/pull/979) test(layout): shape-memo tests hold on Linux too (#974 follow-up)
- [#980](https://github.com/hiwavebrowser/hiwave-macos/pull/980) test: portable pins for relative offsets, inherit, flex, scroll helpers, border medium
- [#996](https://github.com/hiwavebrowser/hiwave-macos/pull/996) test: portable pins for hash no-op, geometry, white-space, var unset
- [#1008](https://github.com/hiwavebrowser/hiwave-macos/pull/1008) fix(real-window): i0a resize asserts hwdrive's own ack; i0a_nav drops Cmd+T (T36)
- [#1009](https://github.com/hiwavebrowser/hiwave-macos/pull/1009) test(app): close-tab tests pick the background tab explicitly (M1 flake)
- [#1014](https://github.com/hiwavebrowser/hiwave-macos/pull/1014) test: portable pins for M2 Drop, B18b border, B25 column margin, E17 disabled, LAT15 mid-load
- [#1021](https://github.com/hiwavebrowser/hiwave-macos/pull/1021) ci(parity): cancel superseded PR runs (T42)
- [#1028](https://github.com/hiwavebrowser/hiwave-macos/pull/1028) ci: move macOS jobs off the retired macos-14 image
- [#1032](https://github.com/hiwavebrowser/hiwave-macos/pull/1032) ci: Parity Gate green — wgate timing tests pin accounting, two stale/regressed script guards (CI2)

### Other (documentation) (2)

- [#795](https://github.com/hiwavebrowser/hiwave-macos/pull/795) docs: README nits from #793 review
- [#1046](https://github.com/hiwavebrowser/hiwave-macos/pull/1046) docs: rewrite CLAUDE.md for the current tree; add the ownership directive
