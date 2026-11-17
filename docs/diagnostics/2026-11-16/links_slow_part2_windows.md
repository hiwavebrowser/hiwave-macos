# LINKS-SLOW part 2: where a 10 ms link's lateness goes (Windows)

Develop `c3df9b75` (#1216 landed) plus the `HIWAVE_LINKS_TRACE` diagnostic of this branch, Windows 11,
parity profile, `HIWAVE_PACE2_1MS=1` (the process asks Windows for 1 ms timers, so sleeps behave
about as on macOS). Chrome for Testing 148.0.7778.216 (`links_lateness_probe_cft148.json`, 5 runs):
278-281 links in 3000 ms, lateness mean 0.64-0.75 ms. Mac, batch 96b (Atlas): 260-270 links, mean
1.2-1.4 ms.

## The probe on Windows

`frame_rate_tests::measure_the_content_threads_link_lateness` (rAF loop beside the 10 ms chain), 3 runs:

| run | frames | links | mean | median | p90 | max | min |
|---|---|---|---|---|---|---|---|
| 1 | 181 | 274 | 0.93 | 0.7 | 1.7 | 21 | 0 |
| 2 | 180 | 275 | 0.90 | 0.8 | 1.8 | 3.2 | 0 |
| 3 | 180 | 269 | 1.13 | 0.8 | 1.9 | 10.3 | 0 |

## The turn, stage by stage (`HIWAVE_LINKS_TRACE=1`, 1292 turns that ran a timer or frame)

1042 of them followed an exact wait (a timer this turn's own timers set). For those, in ms:

| stage | mean | median | p10 | p90 |
|---|---|---|---|---|
| `late`: the turn began past the due instant the loop waited for | **0.812** | 0.728 | 0.148 | 1.614 |
| `aim`: how early the sleep was aimed (`OvershootEstimate`) | 0.811 | 0.853 | 0.506 | 1.052 |
| `entry` + `pre`: loop wake to the timer slice | 0.006 | 0.003 | 0.002 | 0.013 |
| `slice`: timers, then the frame round (1 callback: 0.203; 2 or more: 0.353) | 0.212 | 0.178 | 0.106 | 0.358 |
| `post`: network, layout, paint after the slice | 0.329 | 0.276 | 0.170 | 0.455 |

The sleep itself ended `late + aim` past where it was aimed: mean **1.62 ms**, p5 0.86, median 1.52,
p95 2.79, spread evenly over about a millisecond (250 µs buckets: 0.75-2.0 ms hold 76%). No turn
began before its due instant (the `after_wake` rest held), so nothing ran early.

So of the probe's ~0.9-1.1 ms mean, about 0.8 ms is the wake (the sleep's overshoot that the aim
does not take back), about 0.1-0.2 ms is script before the link's callback inside the slice, and
the loop's own fixed costs between waking and the timer slice are a few microseconds. Frame work
runs after the timers in the slice, and layout and paint after the slice: neither delays a link
that is due at the turn's start. The `post` stage moves when the next wait is computed, not when
it ends (the deadline is anchored at the clock's turn time).

## Why the wake is late: the runtime's timer, not the OS

`frame_rate_tests::measure_sleep_overshoot`: 200 sleeps each, aimed 9.0-9.9 ms ahead, on a
current-thread tokio runtime (the content loop's) and with `std::thread::sleep`, in ms:

| | tokio `sleep_until` | `std::thread::sleep` |
|---|---|---|
| 1 ms timers (`HIWAVE_PACE2_1MS`) | mean 1.402, p10 0.738, p50 1.441, p90 2.033, max 2.703 | mean 0.311, p10 0.094, p50 0.327, p90 0.515, max 0.735 |
| Windows default timers | mean 11.943, p10 11.064, p50 11.960, p90 12.771 | mean 0.287, p10 0.074, p50 0.298, p90 0.493 |

tokio's time driver keeps whole-millisecond deadlines: a deadline is rounded up to its next
millisecond tick and the park waits whole milliseconds, so a sub-millisecond aim cannot land and
the overshoot is spread over a millisecond on top of the OS's own. The estimate in
`OvershootEstimate` follows a low quantile of that spread, so about the spread's mean less its low
end stays in every link. `std::thread::sleep` (a high-resolution waitable timer on Windows,
`nanosleep` on macOS) ends 0.3 ms late here.

A note for hiwave-windows: without 1 ms timers a tokio sleep on Windows ends ~12 ms late.

## What the Mac receipt should look for

Run on the Mac at this branch's head, parity profile:

1. `measure_sleep_overshoot` (no env needed on macOS). Expected if the cause is the same: tokio
   mean ~1.0-1.5 ms spread over ~1 ms; `std::thread::sleep` well under 0.2 ms.
2. `measure_the_content_threads_link_lateness` with `HIWAVE_LINKS_TRACE=1`, and the same summary of
   the `LTRACE` lines (late / aim / entry+pre / slice / post). Expected: `late` carries most of the
   1.2-1.4 ms; `entry + pre` stays in microseconds. If instead `slice` or `post` are large on the
   Mac (frame work or layout on the link's turn), the fix is in the turn, not in the sleep.

## The fix this points to (not in this branch)

Sleep on the runtime until a millisecond or two before the due instant (so sockets and the
runtime's other timers keep their turn), then end the wait with a precise thread sleep (or a
short spin) to the due instant. Expected on Windows: `late` from 0.8 to ~0.3 ms, the probe's mean
from 0.9-1.1 to ~0.5-0.6 ms (Chrome 0.64-0.75), at the cost of blocking the content thread for at
most that last millisecond or two per wait.
