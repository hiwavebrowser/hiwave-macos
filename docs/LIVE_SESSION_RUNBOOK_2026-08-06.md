# Live testing session #2 — runbook (Pete drives, Atlas observes)

> Session #1 (2026-08-05) produced nine defects, all fixed the same night.
> This run verifies those fixes with Pete's hands and finds the next nine.

## Start (Pete, one command)

```bash
cd ~/Repos/hiwave-macos && git pull && ./scripts/pete-session.sh --release
```

`--release` must be the FIRST argument. Session #1 silently ran a debug
build for its first 40 minutes, which cost an hour to a phantom "it's
frozen" that was really 100% CPU laying out eBay.

## What changed since session #1

| Symptom Pete hit | State now | PR |
|---|---|---|
| **Screen frozen on one frame for 20 min** (the big one) | Fixed — surface reconfigures on Outdated/Lost, and any render failure now logs | #100 |
| No scrolling at all | Fixed — wheel, arrows, PageUp/Down, Space, Home/End | #100, #102 |
| Links did nothing | Fixed — left click navigates | #103 |
| Grids of literal "Button" | Fixed — buttons render their children | #100 |
| Wikipedia/eBay bot-walled (429, challenge) | Improved — Safari-shaped UA | #100 |
| 69-second Wikipedia load | Fixed — images/CSS fetch concurrently | #101 |
| Every SVG logo missing | Fixed — rustkit-svg wired into `<img>` | #101 |
| Sidebar clipped at 147px | Fixed — restore clamps to 180px | #100 |
| End key inserted a tofu box | Fixed — caret moves instead | #100 |
| URL bar snapped back to old URL | Fixed — tab model updates before load | #101 |

## Landed after this runbook was first written (least verified — test first)

| Capability | Verified how far |
|---|---|
| **Typing into web forms** | Model + plumbing tested; **no human has seen a character appear.** Click a text field on a real page, type, watch for characters and caret. |
| **Click-to-focus** | Focus resolves in tests through the production layout path; clearing-on-background-click is pinned. |
| Click gate bounds, concurrent SVG | Compile + suite only; behavioral change is invisible when correct. |

The form-typing path is the single biggest unknown in the build. Everything
else tonight was fixing something Pete had already seen fail; this one is new
capability that has never met a human.

## The checklist (each step answers a named question)

1. **Launch.** Sidebar open and readable? Tab strip populated? URL bar showing
   the active tab's URL?
2. **Scroll a long page** — wheel first, then arrow keys, then Space/PageDown,
   then End. Expected: it scrolls. Report *feel*: smooth, steppy, or laggy?
   (Queued lag is expected under heavy layout until the engine-thread
   refactor; that's data, not a surprise.)
3. **Click a link.** Expected: navigates. Try one whose click target is an
   image or a nested span — those exercise the nearest-link resolution.
3b. **Click a text field and type** (a search box on any page). Expected:
   characters appear, caret advances, Backspace deletes, arrows move the
   caret. Then **click away and type again** — the page should scroll, not
   the field, because clicking a non-focusable element clears focus.
   If nothing appears: say so and try clicking dead-centre of the field —
   the hit test uses the border box, and a mis-measured control would be the
   first suspect.
4. **Type a URL, press Enter.** Then check the bar still shows it after the
   page lands (that was the snap-back bug).
5. **Walk the saved tabs.** One verdict each: *looks right / wrong (describe
   one thing) / blank / error page.*
6. **Load a media-heavy page** (Wikipedia article). Watch for: logos present
   (SVG), thumbnails present (UA), and whether it finishes in seconds.
7. **Free play** — the triage catches what narration misses.

## What Atlas watches for live

- Any `View render failing` warn — that's the new instrument; it means a
  frozen screen is happening AND is visible this time
- `Link clicked` lines vs. what Pete says happened (click accuracy)
- Scroll event delivery vs. perceived smoothness
- New HTTP failure classes per host (bot-wall list for cookie-jar work)

## Known limits going in

- **No middle-click / cmd-click to open in a new tab** — left click only.
- **No text selection or form typing in content** — the content NSView still
  has no first-responder wiring; only window-level keys reach the engine.
- Keys typed into UI-frame fields belong to WebKit and must NOT scroll the
  page. If they do, that's a leak worth reporting immediately.
- No print pipeline (fleet-wide: it's a project, not a task).
- Inspector displays nothing (transport wired, display not).

## After the session

Atlas: triage → prioritized list → PRs. If this run is clean enough,
the community-testing call is the next gate per Pete's plan.
