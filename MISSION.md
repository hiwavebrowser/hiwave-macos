# Mission

> My ultimate goal is to create a brand new web browser, from scratch, that will
> replace Chrome as the number one browser. A free and open source browser built
> on Rust, outside the Google food chain, that is more secure, allows private
> browsing, and gives the user more control over what they see, reducing pop-ups
> and trackers as needed. I want to replace Chrome by being just as good as it
> is, and better.
>
> — Pete Copeland, founder

HiWave is built by one person directing a team of AI agents. That's part of
the story too.

## The pillars

1. **Our own engine, from scratch.** Every page is rendered by RustKit, which is
   written in Rust. No Chromium, WebKit or system webview underneath. The end
   state is 100% RustKit on Windows, macOS and Linux, and deleting the borrowed
   webview dependencies is the proof.
2. **Free and open source, outside Google's ecosystem.** MPL-2.0, and no
   Chromium dependency.
3. **More secure and private by default.** Tracking protection and privacy are
   foundations, not add-ons. We don't collect your data.
4. **The user controls what they see.** Pop-ups, trackers and ads are reduced
   as the user chooses.
5. **As good as Chrome, and better.** The parity half is measured against a
   pinned Chrome. The "better" half is pillars 3, 4 and 6.
6. **A calmer browser.** HiWave helps you close tabs, not open more: the
   Shelf, Tab Decay, Workspaces and three levels of automation. Chrome
   optimises for time-in-browser. HiWave optimises for your attention.

## Principles that stay

- **No extensions.** What people use plugins for is built in. We won't clutter
  the browser with third-party add-ons.
- **Supported sites are supported as they are.** The popular sites on our
  boards get whatever compatibility work they need, old hacks included.
  Otherwise we target the modern web.
- **No search partnerships planned**, and no selling data or ads, ever.

## Platforms

macOS first: it's the reference engine tree and the only platform with
pixel-level parity capture today. Windows second, following macOS closely.
Linux later: it shares most of the engine and is expected to track macOS
closely once the Linux seat is running.

## Two tracks, run in parallel

| Track | Question | How we measure it |
|---|---|---|
| **Parity**: "just as good" | Does a real site look and work like it does in Chrome? | The real-site board: the top 20 sites scored for loads, readable and looks right against pinned Chrome for Testing 148 (`scripts/realsite_board.py`). A weekly wide board covers 80 sites. The fixed-page parity campaign is the regression guard. |
| **Better**: privacy, security, control | Does HiWave protect and empower the user more than Chrome does? | Trackers and pop-ups blocked on the wide list, storage and cache partitioning, a referrer policy no looser than Chrome's, and user-facing controls. |

The "better" track doesn't wait for parity to finish. It's the reason
to switch while parity is still being earned.

It also doesn't start from zero:
- **Flow Shield** blocks ads and trackers with Brave's `adblock-rust`, and in
  RustKit mode it blocks at the engine, before the request leaves the browser.
- **Flow Vault** keeps passwords local, with AES-256 encryption.
- Analytics are local-only.

The work is to measure these, extend them (pop-ups, per-site cache and storage
partitioning, the referrer policy) and give the user clear controls.

## Finish line (proposed, to be ratified)

- RustKit is the default renderer on macOS and Windows, with no borrowed
  webview underneath. Linux follows.
- The real-site board reaches **40 of the scorable points** (scorable
  excludes sites that block the reference Chrome itself). The wide board is at
  **75%**. Both are measured against the pinned Chrome.
- Privacy by default: trackers and pop-ups blocked at least as well as Brave's
  lists allow, and the cache and storage partitioned per site.

Progress is reported weekly. Every number carries its receipt: the command,
the commit and the run.
