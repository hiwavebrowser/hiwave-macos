# Parity Progress Report

Generated: 2026-01-06T23:07:46.758986

## Current Status

- **Estimated Parity**: 26.0%
- **Tier A Pass Rate**: 0.0%
- **Tier B Mean Diff**: 74.0%
- **Sparkline**: ▁▁

### Issue Clusters

- sizing_layout: 1736
- paint: 0
- text: 0
- images: 0

## Historical Trend

| Date | Parity | Tier A | Tag | Commit |
|------|--------|--------|-----|--------|
| Jan 06 23:06 | 26.0% | 0.0% | test-comparison | 179647d3 |
| Jan 06 23:05 | 26.0% | 0.0% | initial-baseline | 179647d3 |

## Best / Worst Runs

- **Best**: 20260106_230522 at 26.0% (initial-baseline)
- **Worst**: 20260106_230522 at 26.0% (initial-baseline)

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
