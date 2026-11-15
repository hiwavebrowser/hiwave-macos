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

## Load

The Mac's rule (2026-11-11): a run starts only below a 1-minute load of 6,
and its score counts only if it started below 3. `run.py` waits for the load
to come under `--max-load` (default 3; up to 6 for a run that only looks for
where a suite stops) for `--load-wait` seconds, then gives up with exit code
2. Every record carries `load_avg_before`; `report.py` leaves out any run
that started at 3 or more and counts it in its `disc` column. One suite run
at a time, and none beside a cargo job. Do not `nice` a suite run: a niced
run measures the scheduler, not the browser (builds are niced; benchmarks
are not).

## The run record

One JSON file per run in `--out` (default `~/zg-measure/stress-runs`):
`suite`, `suite_sha`, `engine`, `binary`, `binary_sha1`, `label`, `status`,
`score`, `sub_scores`, `wall_s`, `load_avg_before`/`after`, `errors` (what
the page threw or logged with `console.error`), `last_progress`,
`log_findings` (lines of the app's log that say a script was stopped or the
process died), `off_machine`, `offline_checks` and, when the run did not
finish, `log_tail`.

`status`: `ok` (a score was read), `invalid` (the run was not offline), `failed` (the suite itself reported a
failure), `hung` (nothing from the page for `--stall` seconds), `timeout`,
`exited` (the engine process ended first), `interrupted`.

Scores are upstream's own, higher is better: Speedometer's `Score` (its
`sub_scores` are per-workload milliseconds, lower is better), MotionMark's
score with its bootstrap bounds in `detail`, JetStream's geometric mean of
the per-benchmark scores in its `/report` body.

## Offline

Offline is a condition of a run, not a note on it. Anything seen leaving
this machine ends the run as `invalid`: exit code 1, `score` null (what it
read is kept as `discarded_score`), and `off_machine` says what was seen and
how. How each engine is observed:

- **chrome**: every non-loopback request goes to a local sink proxy that
  refuses and records it. What Chrome asks for itself whatever page is open
  (sign-in, component update, autofill hosts) is listed apart in
  `browser_own_requests`: the sink refused those too, so they did not leave
  the machine. Anything else the sink saw is the page's and invalidates.
- **hiwave** (real window) and **hiwave-headless**: the engine is started
  under a seatbelt profile (`sandbox-exec`, `SANDBOX_PROFILE` in `run.py`)
  that refuses every outbound connection that is not to this machine, the
  resolver included. So the run cannot leave the machine, however short the
  attempt. That it *tried* is read from the engine's own request log
  (`HIWAVE_CONSOLE_LOG=1`: one `net start GET <url>` line per request,
  script `fetch()`/XHR included) and from every other URL in its log. The
  process's sockets are also sampled once a second (`lsof -i`); a sample
  lsof did not answer is counted as failed, never as clean, and a run with
  no answered sample is `invalid` (`unobserved`). Headless runs left to
  finish (`--until-exit`) check the event log's request origins too.
  Limits: the sandbox holds the engine's process and what it forks, not
  another service's process. The app's WebKit views
  (`HIWAVE_RUSTKIT_CHROME=0`) fetch through `com.apple.WebKit.Networking`,
  so `run.py` forces the RustKit chrome on and applies the request-log
  settings last, after `--env` and whatever the shell had (it also drops
  `HIWAVE_CONTENT_THREAD`, which the chrome depends on). A run whose own
  log has no `net start` line for the runner page, or whose app did not
  start with `Chrome: RustKit`, is `invalid` (`unobserved`). The request
  log costs a log line per request, on both arms alike.
- **positive control**: `--suite control` serves a page that tries one
  off-machine fetch (TEST-NET-3) and then reports a score. It must end
  `invalid` on every engine; run it when the harness or the app's logging
  changes. (2026-11-11, develop 48b1191f: `invalid` on chrome, headless
  and the real window.)
- **the app's own start-up**: on a profile with no fresh filter-list cache
  HiWave downloads EasyList and EasyPrivacy when it starts. `run.py` gives
  its throwaway profile this machine's cached copies
  (`easylist.txt` and `easyprivacy.txt` from
  `~/Library/Application Support/hiwave/filter-lists` or
  `$STRESS_FILTER_LISTS`: those two names only, real files only, links not
  followed; public lists, nothing personal) so no request is
  made, and refuses to start (exit 2) when there is no cache to copy. Both
  launches of the app (the one that writes its first-run state and the one
  on the runner page) are watched.

The suites themselves reference no off-machine subresource at these pins
(0 `script`/`img`/`link`/`iframe` tags with an absolute URL).

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
both at the same N on the same machine in the same session. Exit code 0
only when every suite either label has runs for was compared and came out
within noise or improved. Every other outcome is a finding and exits 1: a
regression, a base with no score (none ran, none finished, or all were
discarded for load), a candidate with no score, a candidate that failed
any of its runs, a different N on the two arms. Nothing compared at all (a
mistyped label) exits 2. A finding is read before it is rerun.

Real-window runs also re-check the 17:00-19:30 ET window after every wait
(idle seat, load) and once more immediately before the app starts, and
refuse when now plus the run's wall limit reaches 17:00.
