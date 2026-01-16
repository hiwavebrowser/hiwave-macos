# Chrome Baselines for Visual Parity Testing

This directory contains Chrome-rendered baselines used as ground truth for RustKit visual parity testing.

## Directory Structure

```
baselines/
├── chrome-120/              # Chrome version (update when Chrome updates)
│   ├── builtins/           # Built-in pages (60% weight)
│   │   ├── new_tab/
│   │   │   ├── baseline.png
│   │   │   ├── computed-styles.json
│   │   │   └── layout-rects.json
│   │   ├── about/
│   │   ├── settings/
│   │   ├── chrome_rustkit/
│   │   └── shelf/
│   ├── websuite/           # Websuite cases (40% weight)
│   │   ├── article-typography/
│   │   ├── card-grid/
│   │   ├── css-selectors/
│   │   ├── flex-positioning/
│   │   ├── form-elements/
│   │   ├── gradient-backgrounds/
│   │   ├── image-gallery/
│   │   └── sticky-scroll/
│   └── micro/              # Micro-tests for specific features
│       ├── specificity/
│       ├── combinators/
│       ├── pseudo-classes/
│       ├── intrinsic-sizing/
│       └── ...
├── metadata.json           # Baseline metadata (version, date, environment)
└── README.md               # This file
```

## Baseline Files

Each case directory contains:

- **baseline.png**: Chrome screenshot at specified viewport size
- **computed-styles.json**: Computed CSS properties for key elements
- **layout-rects.json**: DOMRect (getBoundingClientRect) for all elements

## Regenerating Baselines

```bash
# Regenerate all baselines
cd tools/parity_oracle && npm install
node run_oracle.mjs capture --scope all --output ../../baselines/chrome-120

# Regenerate specific case
node run_oracle.mjs capture --case css-selectors --output ../../baselines/chrome-120/builtins
```

## Comparing Against Baselines

```bash
# Run parity baseline with oracle comparison
python3 scripts/parity_baseline.py --oracle chromium --oracle-scope builtins

# Compare single case
node tools/parity_oracle/run_oracle.mjs compare --case new_tab
```

## Version Policy

- Baselines are pinned to a specific Chrome version
- Update baselines when Chrome updates (quarterly)
- Document all baseline changes in git commit messages
- Require team review for baseline updates

## Environment Requirements

- Chrome version: 120.x (or as specified in metadata.json)
- macOS: 14.x (Sonoma)
- DPI: 1x (non-Retina for consistency)
- Font rendering: Default macOS settings



