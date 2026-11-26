# Registered custom properties in WPT

The TRANS-F implementation added 306 passing subtests on its merge candidate. On November 25, HiWave passed 347 of 987 reported subtests across the same 112 pages. Pinned Chrome 148 passed 1,279 of 1,361. Two Chrome pages crashed in the original run and again when retried serially.

| Browser or HiWave commit | Subtests run | Pass | Fail | Timeout | Pass rate | Page OK | Page error | Page timeout | Page crashes |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Before TRANS-F, `c74e7359` | 813 | 43 | 769 | 1 | 5.29% | 70 | 41 | 1 | 0 |
| TRANS-F implementation, `9674293b` | 987 | 349 | 637 | 1 | 35.36% | 99 | 12 | 1 | 0 |
| November 24 develop, `e675eeed` | 987 | 347 | 639 | 1 | 35.16% | 99 | 12 | 1 | 0 |
| November 25 develop, `a7795403` | 987 | 347 | 639 | 1 | 35.16% | 99 | 12 | 1 | 0 |
| Chrome for Testing `148.0.7778.216` | 1,361 | 1,279 | 82 | 0 | 93.98% | 110 | 0 | 0 | 2 |

The pass rate divides passing subtests by the number the browser reported. Fewer HiWave pages ended in a harness error after registration landed, so the number of reported subtests rose by 174. The last four columns account for all 112 pages in each run. Two subtests that passed at `9674293b` failed at `e675eeed`: one compares animated font-relative lengths, and one checks fallback during a root-font-size dependency cycle. This comparison alone does not identify which intervening change affected them.

### Coverage and reproduction

The WPT pin was `a24eb4e7a77917cfa5a6f64e8b02288ca1df1877`. The directory contained 112 testharness pages, seven reference tests, seven crash tests, and twelve other files. The table covers the 112 testharness pages. The separate headless TRANS-F suite passed all 18 tests on `e675eeed`, including the painted gradient midpoint and Tailwind gradient stops.

Build each HiWave host with `cargo build --locked --profile parity -p parity-capture` at the listed commit. On Windows x86-64, the measurements used the repository's loopback handler and `run_page` result decoder, a fresh child per page, a 30-second page timeout, and up to three concurrent children per browser. The same page list can be run sequentially with these commands in Bash. Set `WPT_ROOT` to the pinned upstream WPT checkout, `HOST` to the built HiWave executable, and `PARITY_CHROME_PATH` to the pinned Chrome executable.

```bash
mapfile -t pages < <(
  cd "$WPT_ROOT" &&
  rg --files-with-matches --fixed-strings '/resources/testharness.js' css/css-properties-values-api \
    -g '*.html' -g '*.htm' -g '*.xhtml' \
    -g '!**/resources/**' -g '!**/support/**' -g '!**/crashtests/**' | sort
)
python scripts/wpt_testharness.py --root "$WPT_ROOT" --host "$HOST" --timeout 30 --output hiwave.jsonl "${pages[@]}"
python scripts/wpt_chrome.py --root "$WPT_ROOT" --timeout 30 --output chrome.jsonl "${pages[@]}"
```

The implementation and original Chrome-pinned transition tests landed in [#1374](https://github.com/hiwavebrowser/hiwave-macos/pull/1374). The paint-only transition optimization followed in [#1382](https://github.com/hiwavebrowser/hiwave-macos/pull/1382).
