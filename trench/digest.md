# Trench digest — MCP Phase 1 engine exports

One entry per night. Metric: engine export coverage (see `BASELINE.md`).
Noon digest only — no pings between.

---

## 2026-07-29 — night 0 (baseline pinned, no code)

**Metric: 1 of 4 → 1 of 4** (baseline night, nothing claimed)

- Pinned the metric and stop condition in `BASELINE.md` on Pete's go.
- Verified the one existing answerable tool rather than assuming it:
  `crates/hiwave-mcp/smoke.py` passes and asserts `.hero border_box =
  432.0x152.0` from `hiwave_layout` — a value derivable by hand from the
  fixture CSS (400+2×16 × 120+2×16), so the assertion proves the engine
  computed it rather than that the call returned something.
- **Cannot yet answer:** display list, computed style + winning rule, staged
  diff. Three of four.
- **Decisions needed from Pete:** none.

First working night targets `hiwave_display_list`.
