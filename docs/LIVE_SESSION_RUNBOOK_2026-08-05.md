# Live testing session — runbook (Pete drives, Atlas observes)

> Target: tomorrow evening, Pete's window. One command to start; everything
> else is Pete using the browser like a human while Atlas reads the log live.

## Start (Pete, one command)

```bash
cd ~/Repos/hiwave-macos && git pull && ./scripts/pete-session.sh --release
```

Atlas tails `hiwave-sessions/<ts>/run.log` live from the session. On quit,
`triage.md` lands automatically.

## What changed since Pete's last run

| Symptom from last run | State now |
|---|---|
| `Buffer 'Color Vertex Buffer' … exceeds maximum` repeating, blank tabs | **Fixed** (#90 viewport strip culling) — should not appear at all |
| Form controls / transparent backgrounds | **Fixed** (#83 painter + #91 resurrecting its dead engine half) |
| Sidebar "does not load" | **Fixed** (#93) — opens at saved width (147px) on launch, verified in smoke log |
| URL bar shows nothing | **Half-fixed** (#94) — active tab URL now displays; typing is tonight's open question |
| Slow history pages | Partially the buffer bug; scroll absence is the remaining driver |

## The ordered checklist (each step answers a named question)

1. **Launch.** Does the sidebar appear open at ~147px? (#93 verification by eye.)
2. **Look at the URL bar** on the restored active tab — does it show the URL? (#94.)
3. **Click into the URL bar and type.** If characters appear: the "can't type"
   half was the display bug. If not: say "typing dead" out loud and keep going —
   the log + focus behavior tells Atlas which layer eats keys.
4. **Scroll on any loaded page.** Nothing will scroll (known: content view has
   zero input wiring). The question is whether `window-level MouseWheel received`
   appears in the log (#95 diagnostic). One flick answers where input dies.
5. **Walk the saved tabs one by one.** For each, one verdict out loud:
   - loads and looks right / loads but wrong (describe one thing) / blank / error page
   - Expected buckets from the smoke: eBay-class tabs **403 at the network layer**
     (bot walls — engine-side UA/headers work, not rendering); x.com-class load.
6. **Open `about`** — the known-good page; confirms the baseline still holds.
7. **Anything Pete wants** — free play is the point; the triage catches what
   narration misses.

## What Atlas watches for live

- Any `ERROR` class not in the known list (buffer overflow should be gone —
  its reappearance is a stop-and-look)
- `window-level MouseWheel received` on step 4
- Per-view error attribution for any blank tab (load-fail vs render-fail)
- 403/HTTP failures per tab → builds the bot-wall list for the UA/headers work

## Known limits going in (so nothing reads as a surprise)

- No scrolling anywhere (input wiring is the next engineering unit; the
  diagnostic decides the entry point).
- eBay/chrono24/autotrader tabs will 403 until the network stack sends
  browser-plausible headers — that work is queued behind this session's data.
- The `data:` PDF tab has no PDF path at all; expected blank.
- Inspector shows nothing (transport wired, display not — known from warning
  triage).

## After the session

Atlas: triage → one prioritized bug list → next PRs, community-corpus fixtures
for any page-shaped findings, and the go/no-go read on the community call gate
(tabs + chrome UI) per Pete's plan.
