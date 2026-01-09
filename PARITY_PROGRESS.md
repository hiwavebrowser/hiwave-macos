# Parity Progress Report

Generated: 2026-01-09T10:10:13.216420

## Current Status

- **Estimated Parity**: 98.7%
- **Tier A Pass Rate**: 100.0%
- **Tier B Mean Diff**: 1.3%
- **Sparkline**: ▂▂▁▅▅▇▇▇▇▇

### Issue Clusters

- sizing_layout: 0
- paint: 0
- text: 0
- images: 0

## Historical Trend

| Date | Parity | Tier A | Tag | Commit |
|------|--------|--------|-----|--------|
| Jan 09 10:09 | 98.7% | 100.0% |  | a219e460 |
| Jan 08 23:28 | 98.7% | 100.0% | comment-fix | a219e460 |
| Jan 08 23:18 | 88.7% (-10.1%) | 100.0% | bundled-fonts | a219e460 |
| Jan 08 23:07 | 88.7% | 100.0% |  | a219e460 |
| Jan 08 23:03 | 88.7% | 100.0% | absolute-stretch-fix | a219e460 |
| Jan 08 23:00 | 88.7% | 100.0% | grid-height-fix | a219e460 |
| Jan 08 22:55 | 88.7% | 100.0% | flex-cross-size-fix | a219e460 |
| Jan 08 22:55 | 88.7% | 100.0% |  | a219e460 |
| Jan 08 22:47 | 88.7% | 100.0% | css-math-functions | a219e460 |
| Jan 08 22:44 | 88.7% | 100.0% | baseline-fix | a219e460 |
| Jan 08 22:31 | 88.7% | 100.0% |  | a219e460 |
| Jan 08 22:29 | 88.7% | 100.0% |  | a219e460 |
| Jan 08 22:20 | 88.7% | 100.0% |  | a219e460 |
| Jan 07 21:57 | 88.7% | 100.0% |  | 80eb2e42 |
| Jan 07 21:51 | 88.7% | 100.0% |  | 80eb2e42 |
| Jan 07 21:44 | 88.7% | 100.0% |  | 80eb2e42 |
| Jan 07 21:44 | 88.7% | 100.0% |  | 80eb2e42 |
| Jan 07 21:38 | 88.7% | 100.0% | Testing | 80eb2e42 |
| Jan 07 20:16 | 88.7% | 100.0% | after-phase-a-fix | bb04508f |
| Jan 07 08:41 | 58.9% (-29.8%) | 5.0% |  | 3a3caf34 |
| Jan 07 08:33 | 0.0% (-58.9%) | 0.0% |  | cc8c3f5b |
| Jan 07 08:32 | 0.0% | 0.0% |  | cc8c3f5b |
| Jan 07 08:22 | 58.9% (+58.9%) | 5.0% |  | 8a4144d1 |
| Jan 07 08:21 | 58.9% | 5.0% |  | 8a4144d1 |
| Jan 07 00:19 | 55.2% (-3.7%) | 0.0% | sprint-1.1-flex-intrinsic-fix | 61a4c38f |
| Jan 07 00:00 | 0.0% (-55.2%) | 0.0% | sprint-1.1-inline-fix | 61a4c38f |
| Jan 06 23:55 | 0.0% | 0.0% | sprint-start | 61a4c38f |
| Jan 06 23:44 | 33.1% (+33.1%) | 0.0% |  | 61a4c38f |
| Jan 06 23:30 | 25.8% (-7.3%) | 0.0% |  | f7bda1a0 |
| Jan 06 23:26 | 25.8% | 0.0% |  | f7bda1a0 |
| Jan 06 23:10 | 25.8% | 0.0% |  | daa8a4c8 |
| Jan 06 23:06 | 26.0% (+0.2%) | 0.0% | test-comparison | 179647d3 |
| Jan 06 23:05 | 26.0% | 0.0% | initial-baseline | 179647d3 |

## Best / Worst Runs

- **Best**: 20260108_232854 at 98.7% (comment-fix)
- **Worst**: 20260106_235556 at 0.0% (sprint-start)

## Most Improved Cases (Overall)

- gradient-backgrounds: 85.6% -> 0.3% (-85.3%)
- new_tab: 82.5% -> 0.4% (-82.1%)
- shelf: 86.4% -> 6.9% (-79.5%)
- flex-positioning: 77.6% -> 0.2% (-77.4%)
- settings: 75.0% -> 0.1% (-74.9%)

---

## How to Update

```bash
# Run a new baseline capture
python3 scripts/parity_baseline.py --tag "description"

# Compare to previous run
python3 scripts/parity_compare.py

# Regenerate this report
python3 scripts/parity_summary.py
```
