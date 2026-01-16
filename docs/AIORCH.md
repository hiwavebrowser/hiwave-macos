# AI Orchestrator for RustKit Rendering

This document explains how to use the AI orchestrator tooling to drive the RustKit rendering fix plan with repeatable, artifact-producing test runs.

## Overview

The `tools/ai-orchestrator/aiorch.py` script provides:

- **Local CI emulation**: Run acceptance gates for work orders and produce signed verification reports
- **Canary runner**: Build and run `hiwave-smoke` to validate RustKit rendering with frame capture
- **Roadmap validation**: Verify the work order dependency graph is consistent
- **Metrics tracking**: Record run outcomes for throughput analysis

## Quick Start

### 1. Validate the roadmap

```bash
python tools/ai-orchestrator/aiorch.py validate-roadmap
```

This checks that `.ai/roadmap_index.json` is structurally valid and all work order dependencies exist.

### 2. Run the canary (RustKit smoke test)

```bash
python tools/ai-orchestrator/aiorch.py canary run --profile release --duration-ms 2000 --dump-frame
```

This:
- Builds `hiwave-smoke` (a stress test harness that uses RustKit for content)
- Runs the harness with scripted layout changes
- Captures a frame dump (PPM) for visual verification
- Produces a canary report under `.ai/reports/`

### 3. Run CI gates for a specific work order

```bash
python tools/ai-orchestrator/aiorch.py ci run --work-order prove-pixels
```

This:
- Reads the work order from `.ai/work_orders/prove-pixels.json`
- Executes each acceptance gate (build, test, canary, etc.)
- Produces a verification report under `.ai/reports/`

## Work Orders for RustKit Rendering

The following work orders correspond to the RustKit rendering fix plan:

| Work Order | Description | Dependencies |
|------------|-------------|--------------|
| `prove-pixels` | Add debug visual mode + fix macOS coordinate conversion | (none) |
| `viewport-sizing` | Plumb surface size into renderer per render/resize | `prove-pixels` |
| `display-handle-plumbing` | Pass proper RawDisplayHandle to compositor | `viewport-sizing` |
| `dom-body-fix` | Fix Document::body() and tree builder | `prove-pixels` |
| `css-style-tags` | Parse `<style>`, implement selector matching + variables | `dom-body-fix`, `prove-pixels` |
| `coretext-glyphs` | Implement Core Text glyph rasterization | `css-style-tags`, `prove-pixels` |
| `cleanup-tests` | Remove temp logging, add regression tests | `coretext-glyphs` |

## Artifacts

All run artifacts are stored under `.ai/artifacts/<run_id>/`:

- `canary/`: Canary run logs, frame dumps, manifest
- `<work_order_id>/`: CI gate logs per work order

Reports are stored under `.ai/reports/`:

- `<run_id>_canary-runner.canary.json`: Canary health report
- `<run_id>_<work_order_id>.verification.json`: Signed verification report

## Frame Dump

The `--dump-frame` flag tells the canary to capture a PPM image of the RustKit content view. This provides deterministic visual evidence without relying on OS screenshots.

The frame dump path is recorded in the canary report:
```json
{
  "artifacts": {
    "frame_dump": ".ai/artifacts/<run_id>/canary/frame.ppm"
  }
}
```

You can view PPM files with most image viewers or convert them:
```bash
# macOS: open with Preview
open .ai/artifacts/<run_id>/canary/frame.ppm

# Convert to PNG (requires ImageMagick)
convert frame.ppm frame.png
```

## Developer Workflow

### Working on a specific fix

1. Start the work order:
   ```bash
   python tools/ai-orchestrator/aiorch.py repo start --work-order prove-pixels
   ```

2. Make your changes

3. Run the canary to verify:
   ```bash
   python tools/ai-orchestrator/aiorch.py canary run --profile release --dump-frame
   ```

4. Run all gates for the work order:
   ```bash
   python tools/ai-orchestrator/aiorch.py ci run --work-order prove-pixels
   ```

5. If all gates pass, commit and propose:
   ```bash
   python tools/ai-orchestrator/aiorch.py repo commit --work-order prove-pixels
   python tools/ai-orchestrator/aiorch.py repo propose --work-order prove-pixels
   ```

### Debugging failures

- Check gate logs under `.ai/artifacts/<run_id>/<gate_id>.stdout.txt`
- Check frame dump for visual issues
- Use `RUSTKIT_DEBUG_VISUAL=1` env var to enable debug rendering mode

## hiwave-smoke

The `hiwave-smoke` harness exercises RustKit with:

- A scripted layout stress loop (sidebar drag, shelf show/hide)
- RustKit content rendering (instead of WRY)
- Frame capture capability for deterministic testing

```bash
# Build
cargo build -p hiwave-smoke --release

# Run with frame dump
./target/release/hiwave-smoke --duration-ms 2000 --dump-frame /tmp/frame.ppm
```

## Integration with fix_rustkit_rendering plan

This orchestrator tooling directly supports the [fix_rustkit_rendering plan](../.cursor/plans/fix_rustkit_rendering_2f2746c3.plan.md):

1. **Phase 0 (prove pixels)**: Use `prove-pixels` work order, verify via canary frame dump
2. **Phase 1 (DOM body fix)**: Use `dom-body-fix` work order, verify via unit tests
3. **Phase 2 (CSS styles)**: Use `css-style-tags` work order, verify via canary visual output
4. **Phase 3 (Core Text glyphs)**: Use `coretext-glyphs` work order, verify text is readable
5. **Phase 4 (cleanup)**: Use `cleanup-tests` work order, verify all tests pass

Each work order has acceptance gates that must pass before the fix is considered complete.


