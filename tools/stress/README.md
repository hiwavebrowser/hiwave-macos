# Stress category: Speedometer, MotionMark, JetStream

A separate measurement category (Pete, 2026-11-11). Three public benchmark
suites, pinned, served from this machine only, scored per binary. Its
numbers live in their own report (`report.py`); they are not part of the
chrome board, RS1, WPT or the parity campaign.

| suite | pin | runner hook |
|---|---|---|
| Speedometer 3.1 | `WebKit/Speedometer` `release/3.1` @ `1386415b` | `index.html?startAutomatically=true`, `globalThis.benchmarkClient` |
| MotionMark 1.3.2 | `WebKit/MotionMark` tag `release/MotionMark1.3.2` @ `0e740d50` | `MotionMark/index.html` + `benchmarkController.startBenchmark()` (what the Run Benchmark button calls); `developer.html?suite-name=…&test-name=…` for one subtest |
| JetStream 2.2 | `WebKit/JetStream` branch `JetStream2.2` @ `332d8ee1` | `index.html?report=true`: starts itself, POSTs its results JSON to `/report` |

Full shas, licences and build notes are in `pins.json`.

## Files

- `pins.json`: repository, ref, sha and licence note per suite.
- `fetch.sh`: checks the suites out at those shas into `$STRESS_SUITES`
  (default `~/zg-measure/stress-suites`). The only network use here. The
  suites are never committed to this repository. None needs a build step:
  Speedometer commits every workload's `dist/`.
- `run.py`: one run of one suite on one binary; writes one JSON record.
- `reporter.js`: the in-page half. `run.py`'s server adds it to the runner
  page it serves (the checkout on disk is not edited, and `run.py` refuses a
  checkout that is dirty or off its pin). It wraps the suite's own client
  callbacks and POSTs progress, errors and the result to the serving origin.
- `report.py`: median, min-max and N per suite, and base vs candidate.

## Commands

```bash
tools/stress/fetch.sh                                   # once per machine

# HiWave, real window (takes ~/.claude/realwindow.lock, HID idle >= 180 s,
# refuses 17:00-19:30 ET; the app it starts is the only process it stops)
python3 tools/stress/run.py --suite speedometer --engine hiwave --binary target/release/hiwave --label develop
python3 tools/stress/run.py --suite motionmark  --engine hiwave --binary target/release/hiwave --label develop
python3 tools/stress/run.py --suite jetstream   --engine hiwave --binary target/release/hiwave --label develop

# HiWave, no window (parity-capture's live loop)
python3 tools/stress/run.py --suite speedometer --engine hiwave-headless --binary target/release/parity-capture --label develop

# Chrome reference (headless; --headed takes the real-window lock)
CFT="<repo>/.browsers/chrome/mac_arm-148.0.7778.216/chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing"
python3 tools/stress/run.py --suite speedometer --engine chrome --binary "$CFT" --label cft148

# Report
python3 tools/stress/report.py                          # median, min-max, N
python3 tools/stress/report.py --subs                   # with per-subtest medians
python3 tools/stress/report.py --compare develop cand   # exit 1 on a regression
```

Finding where a suite stops:

```bash
# every subtest start is reported; the record's last_progress is where it stopped
python3 tools/stress/run.py --suite speedometer --engine hiwave --binary … --trace
# one workload / one subtest (a partial run: kept apart from full runs in the report)
python3 tools/stress/run.py --suite speedometer --only TodoMVC-JavaScript-ES5 --iterations 1 …
python3 tools/stress/run.py --suite motionmark  --only Multiply --test-interval 10 …
python3 tools/stress/run.py --suite jetstream   --only FlightPlanner …
# headless, with the engine's event log (requests by origin, script errors,
# unsupported APIs): the engine is left to end its own live window
python3 tools/stress/run.py --suite speedometer --engine hiwave-headless --binary target/release/parity-capture \
    --timeout 60 --until-exit --keep
```

## The run record

One JSON file per run in `--out` (default `~/zg-measure/stress-runs`):
`suite`, `suite_sha`, `engine`, `binary`, `binary_sha1`, `label`, `status`,
`score`, `sub_scores`, `wall_s`, `load_avg_before`/`after`, `errors` (what
the page threw or logged with `console.error`), `last_progress`,
`log_findings` (lines of the app's log that say a script was stopped or the
process died) and, when the run did not finish, `log_tail`.

`status`: `ok` (a score was read), `failed` (the suite itself reported a
failure), `hung` (nothing from the page for `--stall` seconds), `timeout`,
`exited` (the engine process ended first), `interrupted`.

Scores are upstream's own, higher is better: Speedometer's `Score` (its
`sub_scores` are per-workload milliseconds, lower is better), MotionMark's
score with its bootstrap bounds in `detail`, JetStream's geometric mean of
the per-benchmark scores in its `/report` body.

## Offline

A run makes no request off this machine:

- the suites reference no external URL from their runner pages or workloads
  at these pins (checked by running them: see below);
- `chrome` runs go through a local sink proxy that refuses and records every
  non-loopback request. `non_loopback_requests` must be empty.
  `browser_own_requests` lists what Chrome asked for itself (sign-in,
  component update, autofill hosts), which the sink refused too;
- `hiwave-headless --until-exit` runs list every request origin from the
  engine's event log in `headless_events.request_origins`;
  `non_loopback_origins` must be empty.

The real-window app is not covered by either check: what the app itself asks
for at start-up is outside this harness.

## Headless or window

`hiwave-headless` runs the same engine, the same live loop pacing as the app
(`parity-capture --live-ms`: timers on the real clock, `requestAnimationFrame`
as a 16 ms frame timer) and the app's 15 s script budget, but presents
nothing: frames are drawn offscreen and never reach a window. It is a fair
arm for script-bound scores (Speedometer, JetStream) and for finding what
blocks a suite. MotionMark measures frames reaching the screen, so its score
of record is the real window's. Do not mix engines in one comparison:
`report.py` keeps them apart.

## Landing rule (proposed)

`report.py --compare BASE CAND`: the candidate regresses when its median is
below the base's median by more than `max(3 %, the base's min-max spread)`,
both at the same N on the same machine in the same session. A suite the base
scores and the candidate does not is a regression. A regression is a landing
finding, read before it is rerun.
