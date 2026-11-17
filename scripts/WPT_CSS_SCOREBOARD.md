# CSS testharness scoreboard

The suite uses the WPT commit in `scripts/fetch_wpt_suite.py`. The preparation command verifies a SHA-256 digest of the full source archive. The inventory uses WPT's testharness classification and manifest URLs, including query and fragment variants and generated pages.

Run from the repository root with Python 3.12+, the parity oracle's Playwright installation, a built parity-capture binary and Chrome for Testing 148.0.7778.216. On Windows:

```powershell
$env:PARITY_CHROME_PATH = 'C:\path\to\chrome.exe'
$local = Join-Path $env:TEMP 'hiwave-css-run'
New-Item -Force -ItemType Directory $local | Out-Null
python scripts/prepare_wpt_css.py "$local/wpt"
python scripts/wpt_css_inventory.py "$local/wpt" | Set-Content -Encoding ascii "$local/pages.json"
cargo build -j 2 -p parity-capture
python scripts/wpt_css_run.py "$local/wpt" "$local/pages.json" target/debug/parity-capture.exe "$local/paired.jsonl" --jobs 4
python scripts/wpt_css_report.py "$local/pages.json" "$local/paired.jsonl" "$local/expectations.json" "$local/clusters.json" --tables "$local/directory-tables.md"
```

The committed directory tables are in [WPT_CSS_RESULTS.md](WPT_CSS_RESULTS.md). The complete ranked list and file sets are in [wpt_css_clusters.json](wpt_css_clusters.json).

The download happens during preparation. Both browsers load from the same loopback origin. WPT's server supplies the generated resources, HTTP headers and substitutions. The runner appends a completion callback to the served harness. Each page gets a fresh browser process. The deadline contains hangs. Keep the JSONL and server logs local. A rerun with the same inputs and output path resumes completed pairs. Use a new output file after changing the host binary, WPT pin or browser.

Rates use the larger reported subtest count per page as the shared denominator. A page with no callbacks in either browser contributes to page and harness-status counts only. The observed counts show how many subtests each browser actually returned. The cluster report considers matching names and occurrence numbers on completed pages where Chrome passed and HiWave failed. Each matched failure contributes to at most one cluster. Assertions that do not identify a cause remain unclassified. Several WPT assertions can stop at the same first failure. Fixing it can expose subsequent failures.
