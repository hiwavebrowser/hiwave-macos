# Inspector fixtures

Inputs for the HiWave Inspector page (`crates/hiwave-app/src/ui/inspector_rustkit.html`)
and its parity case (`inspector_rustkit` in `cases/registry.json`).

| file | what it is |
|---|---|
| `target.html` | the page `crates/rustkit-engine/src/diagnostics_tests.rs` records (its `page()`), as a file |
| `target.planted.dump.json` | `parity-capture --html-file target.html --width 800 --height 600 --dump-diagnostics` on Linux, with one difference planted in `#f2` (`flex-grow` 2, boxes 30px wider), as `tools/inspector`'s tests plant them |
| `f2_planted.diff.json` | `tools/inspector/inspector_diff.py target.planted.dump.json <chrome.json>`, where `chrome.json` is `tools/inspector/chrome_records.mjs target.html` at 800x600 |
| `inspector_page.html` | the Inspector page with the two files above injected: the parity case's fixture |

Regenerate `inspector_page.html` after changing the Inspector page (a test,
`the_parity_case_is_the_template_with_its_fixture_input`, fails until you do):

```bash
cargo build --release -p parity-capture
./target/release/parity-capture --inspector-dump fixtures/inspector/target.planted.dump.json \
  --inspector-diff fixtures/inspector/f2_planted.diff.json --inspector-out /tmp/inspector
cp /tmp/inspector/inspector.html fixtures/inspector/inspector_page.html
```

Then recapture the case's Chromium baseline with the pinned Chrome:
`PARITY_CHROME_PATH=<Chrome for Testing 148.0.7778.216> python3 scripts/generate_baselines.py --case inspector_rustkit`.
