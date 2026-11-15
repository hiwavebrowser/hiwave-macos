# PACE2: the content thread loop's frame rate

Probe: `frame_rate_probe.html`, a requestAnimationFrame loop that moves a box beside a
`setTimeout(link, 10)` chain, for 3000 ms of `performance.now`.

Chrome for Testing 148.0.7778.216, headless, five runs (`frame_rate_probe_cft148.json`):
180-181 frames, 281-283 links.

## The app's loop (`frame_rate_tests`, ignored), develop c5cfddac, parity profile, Windows

The engine test harness runs the page on the content thread (`PageHalf::run`), the loop the app
uses by default (`hiwave-app` sets `HIWAVE_CONTENT_THREAD=1`), so this is the loop MotionMark
runs on in the real window. Three runs each.

Windows sleeps in 15.6 ms ticks unless the process asks for 1 ms (`timeBeginPeriod(1)`,
`HIWAVE_PACE2_1MS=1` in the measure); with 1 ms a sleep lasts about what it does on macOS. The
Mac app's own probe (Atlas, batch 86 head): 51.4-53.9 frames per wall second, debug equal to
release.

| loop | timer | frames / 3000 ms | links / 3000 ms |
|---|---|---|---|
| develop | 1 ms | 160, 160, 160 | 282, 282, 282 |
| develop | 15.6 ms (default) | 107, 108, 110 | 289, 290, 289 |

Per turn, develop, 1 ms timer (a temporary, uncommitted log in `PageHalf::run`; 1141 turns of
the three runs that waited less than 50 ms):

| | mean | median | p90 |
|---|---|---|---|
| ms given to the turn | 7.4 | 7.0 | 11.0 |
| fraction dropped (us) | 460 | 487 | 871 |
| turn + its frame, before the wait (us) | 363 | 270 | 491 |
| wait asked (ms) | 5.5 | 6 | 9 |
| sleep past its time (us) | 1270 | 1223 | |

The page's clock ran at 0.942 of real time (the dropped fractions). About 2.4 turns a frame, so a
frame lost ~1.1 ms to dropped fractions and ~1.6 ms to the turn's own time and the late sleep
(the next frame is due 16 ms after the turn that ran the last, not on a grid): 18.8 ms a frame,
53 a second. With the default 15.6 ms timer a 2 ms wait sleeps ~14 ms and a frame comes every
other turn (36 a second); that is a Windows-only cost, a row for the Windows port, not this one.

## The fix

`LiveClock` keeps the fraction for the next turn and sleeps until `wait` after the turn's time on
the page's clock, not after the turn's end. Model (`live_clock_tests`, 0.4 ms turns, sleeps
1.2 ms late): 165 frames and 276 links before, 179 and 299 after.

The app's loop after the fix, same measure, three runs each:

| loop | timer | frames / 3000 ms | links / 3000 ms |
|---|---|---|---|
| Chrome | | 180-181 | 281-283 |
| develop | 1 ms | 160, 160, 160 | 282, 282, 282 |
| PACE2 | 1 ms | 173, 173, 173 | 296, 297, 296 |
| develop | 15.6 ms | 107, 108, 110 | 289, 290, 289 |
| PACE2 | 15.6 ms | 125, 125, 126 | 296, 297, 296 |

What is left to 180: a frame is due 16 ms after the turn that ran the last one, so each sleep's
lateness (~1.2 ms) still moves every later frame. Chrome's frames are on the display's grid; a late
one does not move the next. That is the next row (frames on a grid), not this one.

The chain now runs 296-297 links where Chrome runs 281-283: on develop a link set in a callback
counts from that callback's due time, so the chain stays on a 10 ms grid however late it runs, and
with the page's clock no longer falling behind it gets every grid point. Chrome counts from when
the link is set (~10.6 ms a link). #1183 (CLOCK3) counts a link set in a live turn from the turn's
time plus the real time it has taken, as Chrome does; the two are to be measured together once
both have landed.
