# Parity 80%+ (Hybrid) Plan

## Goal

Reach **≥80% parity** (targeting 90%+) with fewest cycles by using a hybrid gate:
- **Fast signal**: Heuristic score (layout-driven) for rapid iteration
- **Truth signal**: Chromium pixel diff for ground truth validation
- **Parallel development**: Infrastructure enabling multiple developers to contribute simultaneously

## Success Milestones

| Milestone | Target | Criteria | Status |
|-----------|--------|----------|--------|
| **M1** | 80% parity | Unblock shipping | Current: 58.9% |
| **M2** | 90% parity | All builtins <15% diff | Pending |
| **M3** | 95% parity | Top 5 websuite <10% diff | Pending |
| **Stretch** | 98% parity | Pixel-perfect for common cases | Pending |

---

## Current State (20260107_084119)

- **Estimated parity (heuristic)**: 58.9% (weighted mean diff 41.1%)
- **Worst cases**: css-selectors (59.9%), image-gallery (54.3%), sticky-scroll (53.4%)
- **Best case**: gradient-backgrounds (17.7%)

### Key Measurement Issues (must fix first)

1. **issue_clusters always 0** - `parity_baseline.py` uses `layout_stats` from capture result which lacks per-node issue detail
2. **Noisy zero_size penalty** - Counts any `w==0 OR h==0`; many `width:0, height:line_height` nodes are benign (e.g., text runs)

---

## Phase 0 — Infrastructure for Parallel Development

> Run in parallel with Phase A to enable distributed work immediately.

### 0.1 Shared Baseline Storage

```
parity-baseline/
  chromium-baselines/          # Chrome reference images (git-lfs or S3)
    chrome-130/
      css-selectors.png
      image-gallery.png
      ...
  rustkit-captures/            # Current RustKit output
  diffs/                       # Visual diff images
  computed-styles/             # CSS property snapshots (new)
```

- Host chromium baselines in git-lfs or external storage (S3/GCS)
- Version by Chrome version for reproducibility
- Any developer can pull baselines without regenerating

### 0.2 Case Assignment System

Create `parity-baseline/assignments.json`:

```json
{
  "cases": {
    "css-selectors": {
      "assignee": null,
      "status": "unassigned",
      "started": null,
      "target_diff": 25,
      "current_diff": 59.89,
      "branch": null,
      "notes": "Likely selector matching + cascade issues"
    },
    "image-gallery": {
      "assignee": null,
      "status": "unassigned",
      "started": null,
      "target_diff": 25,
      "current_diff": 54.30,
      "branch": null,
      "notes": "Image intrinsic sizing + object-fit"
    },
    "sticky-scroll": {
      "assignee": null,
      "status": "unassigned",
      "started": null,
      "target_diff": 25,
      "current_diff": 53.38,
      "branch": null,
      "notes": "Scroll offset model + position:sticky"
    }
  },
  "rules": {
    "max_cases_per_developer": 2,
    "rotation_days": 7,
    "regression_threshold_pct": 2
  }
}
```

**Assignment Rules:**
- One primary developer per case to avoid conflicts
- Weekly rotation if no progress (>7 days, <5% improvement)
- Cases can be split into sub-issues (e.g., `css-selectors-specificity`)

### 0.3 Git Workflow for Parallel Work

```
master
  └── parity/css-selectors      (developer-a)
  └── parity/image-gallery      (developer-b)
  └── parity/sticky-scroll      (developer-c)
  └── parity/infrastructure     (shared tooling)
```

**Merge Rules:**
- Must not regress ANY case by >2%
- Must improve target case by ≥5%
- CI runs full oracle suite on PR
- Require sign-off from another parity contributor

### 0.4 Quick Iteration Tooling

Add to `scripts/parity_baseline.py`:

```bash
# Run single case for fast iteration
python3 scripts/parity_baseline.py --case css-selectors

# Run only assigned cases for a developer
python3 scripts/parity_baseline.py --assignee developer-a

# Compare branch against master baseline
python3 scripts/parity_compare.py --branch parity/css-selectors
```

### Acceptance Gate 0

- [ ] `assignments.json` created and documented
- [ ] Single-case `--case` flag working
- [ ] Branch comparison tooling functional
- [ ] At least 2 developers can work on separate cases without conflicts

---

## Phase A — Fix the Measurement Loop

> Fix measurement before optimizing, or we'll optimize the wrong thing.

### A.1 Fix Zero-Size Penalty

Update `scripts/parity_baseline.py`:

**Current (noisy):**
```python
zero_size = w == 0 or h == 0  # Catches benign text runs
```

**Fixed (area-based):**
```python
zero_size = (w * h == 0) and node_type not in ['text_run', 'inline']
```

Options:
1. **(Quick)** Area-only check: `w * h == 0`
2. **(Better)** Area check + node type allowlist for benign zeros

### A.2 Fix Issue Clustering

**Current problem:** `layout_stats` from capture lacks per-node issue detail.

**Solutions:**

| Option | Effort | Accuracy | Description |
|--------|--------|----------|-------------|
| Option 1 | Low | Medium | Always run `analyze_layout(layout_path)` and merge totals |
| Option 2 | Medium | High | Extend layout JSON to include `type` field per node |
| Option 3 | High | Highest | Full computed-style export from RustKit |

**Recommended:** Start with Option 1, plan for Option 2.

### A.3 Add Correlation Validation

Ensure heuristic changes correlate with visual changes:

```python
# In parity_baseline.py
def validate_heuristic_correlation(old_report, new_report):
    """Warn if heuristic moved but visual diff didn't (or vice versa)."""
    for case in cases:
        heuristic_delta = new_report[case].diff - old_report[case].diff
        # Compare against actual pixel diff if available
        if abs(heuristic_delta) > 5 and pixel_diff_delta < 1:
            warn(f"{case}: Heuristic changed {heuristic_delta}% but pixels unchanged")
```

### Acceptance Gate A

- [ ] Heuristic stable (no "all clusters 0" for cases with obvious problems)
- [ ] Heuristic changes correlate with visible PPM changes (>80% correlation)
- [ ] Zero-size penalty only fires on actual layout failures
- [ ] Running same case twice produces same heuristic (±0.5%)

---

## Phase B — Chromium Oracle + Enhanced Tooling

> Add ground truth pixel comparison for top-impact cases.

### B.1 Chromium Oracle Tool

**Location:** `tools/parity_oracle/`

```
tools/parity_oracle/
  ├── package.json           # playwright, pngjs, pixelmatch
  ├── run_oracle.mjs         # CLI entry point
  ├── capture_chrome.mjs     # Playwright-based capture
  ├── compare_pixels.mjs     # pixelmatch wrapper
  └── export_styles.mjs      # Computed style extraction (new)
```

**Capabilities:**
1. Render HTML via Playwright (Chrome)
2. Save `chromium.png` baseline per case
3. Convert RustKit PPM → RGBA
4. Compute pixel diff via pixelmatch
5. Emit `diff_pct` and `diff.png` artifact
6. **(New)** Export computed styles per element

### B.2 Visual Diff Overlay

Generate diff images showing exactly WHERE differences are:

```
parity-baseline/diffs/
  ├── css-selectors.diff.png      # Red = RustKit only, Green = Chrome only
  ├── css-selectors.overlay.png   # Side-by-side with diff highlighted
  └── css-selectors.heatmap.png   # Intensity = magnitude of difference
```

### B.3 Computed Style Comparison (High Value)

For debugging CSS issues, compare computed styles:

```javascript
// export_styles.mjs
async function exportComputedStyles(page, selector = '*') {
  return await page.evaluate((sel) => {
    const elements = document.querySelectorAll(sel);
    return Array.from(elements).map(el => ({
      selector: getUniqueSelector(el),
      styles: {
        display: getComputedStyle(el).display,
        width: getComputedStyle(el).width,
        height: getComputedStyle(el).height,
        // ... key properties
      }
    }));
  }, selector);
}
```

Compare against RustKit's computed styles to find CSS property mismatches.

### B.4 Integration with parity_baseline.py

New flags:

```bash
python3 scripts/parity_baseline.py \
  --oracle chromium \              # Enable Chromium comparison
  --oracle-scope top|builtins|all \ # Which cases (default: top)
  --oracle-threshold 25 \          # Tier A pass threshold
  --diff-images \                  # Generate visual diffs
  --computed-styles                # Export style comparisons
```

**Behavior:**
- Oracle-covered cases: Use real pixel diff as primary
- Other cases: Keep heuristic diff as fast signal
- Store results in `baseline_report.json` with `"source": "oracle"` or `"source": "heuristic"`

### Acceptance Gate B

- [ ] Single command produces for oracle cases:
  - `parity-baseline/oracle/chromium/<case>.png`
  - `parity-baseline/diffs/<case>.diff.png`
  - Real `diff_pct` in `baseline_report.json`
- [ ] Visual diff images clearly show problem areas
- [ ] Computed style diff available for at least top 3 cases
- [ ] Oracle runs in <60s for top 3 cases

---

## Phase C — Targeted Engine Work (Parallelizable)

> Use oracle to avoid guesswork. Each case can be worked on by a different developer.

### CSS Feature Matrix

Track feature-level parity:

| Feature | Test Cases | Current Parity | Priority | Assignee |
|---------|------------|----------------|----------|----------|
| Selector matching | css-selectors | 40% | P0 | - |
| Flex layout | flex-positioning, card-grid | 55% | P0 | - |
| Image sizing | image-gallery | 46% | P0 | - |
| Sticky positioning | sticky-scroll | 47% | P1 | - |
| Gradients | gradient-backgrounds | 82% | P2 | - |
| Form controls | form-elements | 48% | P1 | - |

**Priority Formula:** `priority = (1 - parity) × usage_weight × case_weight`

### C1: css-selectors (59.9% diff)

**Likely Issues:**
- Selector matching gaps (combinators, specificity, pseudo-classes)
- Cascade/inheritance errors
- Missing UA stylesheet defaults

**Debugging Approach:**
1. Run computed style diff → identify which elements have wrong styles
2. Check selector matching for those elements
3. Verify cascade order and specificity calculation
4. Compare against UA stylesheet

**Micro-tests to Create:**
- `websuite/micro/selectors-specificity.html`
- `websuite/micro/selectors-combinators.html`
- `websuite/micro/selectors-pseudo-classes.html`

### C2: image-gallery (54.3% diff)

**Likely Issues:**
- Image decode path + texture upload
- Intrinsic sizing (natural width/height, aspect-ratio)
- `object-fit` defaults and layout interaction

**Debugging Approach:**
1. Compare image element dimensions (RustKit vs Chrome)
2. Check if images are loading at all (vs placeholder)
3. Verify aspect-ratio preservation
4. Test object-fit values

**Micro-tests to Create:**
- `websuite/micro/image-intrinsic-size.html`
- `websuite/micro/image-object-fit.html`
- `websuite/micro/image-aspect-ratio.html`

### C3: sticky-scroll (53.4% diff)

**Likely Issues:**
- Scroll offset not applied in paint/compositor
- `position: sticky` constraint calculation
- Scroll container identification

**Debugging Approach:**
1. Capture at multiple scroll positions
2. Compare sticky element positions at each scroll offset
3. Verify scroll container bounds

**Micro-tests to Create:**
- `websuite/micro/sticky-basic.html`
- `websuite/micro/sticky-nested.html`
- `websuite/micro/scroll-offset.html`

### Acceptance Gate C

For each targeted case:
- [ ] Heuristic improves (or stays stable)
- [ ] Chromium pixel `diff_pct` improves by ≥5%
- [ ] At least 2 micro-tests created and passing
- [ ] No regression in other cases >2%

---

## Phase D — Expand Coverage & Hit 80%

### D.1 Expand Oracle Coverage

Progression:
1. **Top 3 worst** (Phase C) → validate approach
2. **All builtins** (5 cases) → ensure core UI is solid
3. **All websuite** (8 cases) → comprehensive coverage
4. **Micro-tests** → granular feature validation

### D.2 Compute Blended Parity Score

```python
def compute_parity(report):
    oracle_cases = [c for c in report.cases if c.source == 'oracle']
    heuristic_cases = [c for c in report.cases if c.source == 'heuristic']

    oracle_parity = weighted_mean([100 - c.diff_pct for c in oracle_cases])
    heuristic_parity = weighted_mean([100 - c.diff_pct for c in heuristic_cases])

    # Weight oracle higher as it's ground truth
    if oracle_cases:
        return oracle_parity * 0.8 + heuristic_parity * 0.2
    return heuristic_parity
```

### D.3 Regression Prevention

Add CI gate:

```yaml
# .github/workflows/parity-gate.yml
parity-check:
  runs-on: macos-latest
  steps:
    - name: Run parity baseline
      run: python3 scripts/parity_baseline.py --oracle chromium --oracle-scope builtins

    - name: Check for regressions
      run: |
        python3 scripts/parity_compare.py --fail-on-regression 2

    - name: Verify minimum parity
      run: |
        python3 scripts/parity_gate.py --minimum 80
```

### Acceptance Gate D (80% Milestone)

- [ ] `python3 scripts/parity_baseline.py --gpu --oracle chromium --oracle-scope builtins` reports **≥80% parity**
- [ ] Top 3 websuite cases each **≤25% pixel diff** (Tier A threshold)
- [ ] No case has regressed >5% from baseline
- [ ] CI gate passing on all PRs

---

## Phase E — Push to 90%+ and Sustain

### E.1 Subpixel Accuracy Mode

After 80%, differences become subtle:

```bash
python3 scripts/parity_baseline.py \
  --subpixel-tolerance 0.5 \    # Allow 0.5px differences
  --report-soft-diff            # Track "soft" vs "hard" differences
```

**Soft diff:** Within tolerance (acceptable)
**Hard diff:** Outside tolerance (needs fixing)

### E.2 Platform-Specific Baselines

Font rendering differs significantly across platforms:

```
parity-baseline/chromium-baselines/
  ├── macos-arm64/
  ├── macos-x64/
  ├── linux-x64/
  └── windows-x64/
```

Track and report parity per-platform.

### E.3 Text Rendering Deep Dive

For 90%+ parity, text rendering becomes critical:

- Compare CoreText (macOS) metrics directly
- Font metric extraction (ascent, descent, line-height)
- Baseline alignment verification
- Subpixel text positioning

### E.4 Continuous Monitoring

```yaml
# Nightly CI job
nightly-parity:
  schedule: "0 2 * * *"  # 2 AM daily
  steps:
    - run: python3 scripts/parity_baseline.py --oracle chromium --oracle-scope all
    - run: python3 scripts/parity_alert.py --slack-webhook $SLACK_URL --threshold 2
```

**Alerts:**
- Slack/Discord notification on regression >2%
- Weekly parity trend report to team
- Monthly summary with graphs

### E.5 Community Test Contributions

Enable external contributors to add test cases:

```
websuite/community/
  ├── flex-wrap-issue-123/
  │   ├── index.html        # Minimal reproduction
  │   ├── expected.png      # Chrome screenshot
  │   └── metadata.json     # { "author": "...", "css_features": [...], "description": "..." }
  └── ...
```

**Contribution Workflow:**
1. Contributor creates minimal HTML reproducing a parity gap
2. Captures Chrome baseline locally
3. Submits PR with test case
4. CI validates format and adds to suite
5. Case automatically included in next parity run

### Acceptance Gate E (90%+ Milestone)

- [ ] Overall parity **≥90%**
- [ ] All builtins **<15% diff**
- [ ] Top 5 websuite **<20% diff**
- [ ] Nightly CI running and alerting
- [ ] At least 5 community-contributed test cases

---

## Quick Reference: Key Commands

```bash
# Full baseline run (heuristic only)
python3 scripts/parity_baseline.py --tag "description"

# Full baseline with Chromium oracle
python3 scripts/parity_baseline.py --oracle chromium --oracle-scope all

# Single case iteration (fast)
python3 scripts/parity_baseline.py --case css-selectors --oracle chromium

# Compare against previous run
python3 scripts/parity_compare.py

# View historical trends
python3 scripts/parity_summary.py

# CI gate check
./scripts/parity_gate.sh --minimum 80
```

---

## Key Files to Change/Add

### Scripts (Python)
- `scripts/parity_baseline.py` — Add `--case`, `--oracle`, `--diff-images`, `--computed-styles`
- `scripts/parity_compare.py` — Add `--branch`, `--fail-on-regression`
- `scripts/parity_alert.py` — **New**: Slack/webhook notifications
- `scripts/parity_gate.py` — **New**: CI minimum threshold check

### Oracle Tool (Node.js)
- `tools/parity_oracle/package.json`
- `tools/parity_oracle/run_oracle.mjs`
- `tools/parity_oracle/capture_chrome.mjs`
- `tools/parity_oracle/compare_pixels.mjs`
- `tools/parity_oracle/export_styles.mjs`

### Configuration
- `parity-baseline/assignments.json` — Case assignments
- `parity-baseline/config.json` — Thresholds and weights

### RustKit Engine (Optional but recommended)
- Layout JSON export: Add `type` field per node
- Computed style export: Expose resolved CSS values

---

## Notes

- **GPU capture** must run outside sandboxed terminals (use Terminal.app)
- **Chrome version** affects baselines; document which version generated them
- **CI runners** need GPU access for accurate captures (or use `--headless` with caveats)
- **Disk space**: Full oracle suite with diffs ~500MB; consider pruning old runs

---

## Effort Estimates (Not Time)

| Phase | Complexity | Dependencies | Parallelizable |
|-------|------------|--------------|----------------|
| Phase 0 | Low | None | Yes (with A) |
| Phase A | Medium | None | Yes (with 0) |
| Phase B | Medium-High | Phase A | Partially |
| Phase C | High | Phase B | Yes (C1/C2/C3 parallel) |
| Phase D | Medium | Phase C | No |
| Phase E | Ongoing | Phase D | Yes |
