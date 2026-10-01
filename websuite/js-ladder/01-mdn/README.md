# js-ladder rung 1: MDN learning-area

These pages are vendored unmodified from [mdn/learning-area](https://github.com/mdn/learning-area) (CC0-1.0) at the commit pinned in `manifest.json`. The set is the examples that touch the DOM, kept in MDN's course order.

- Re-vendor: `python3 scripts/js_ladder_vendor.py --commit <sha>`
- Score: `python3 scripts/js_ladder.py` (needs `PARITY_CHROME_PATH` set to the pinned CfT 148)

Each page is scored once, after load, against its committed Chrome 148 frame. It passes at <= 15% pixel diff, the campaign's t15 cap. The click-driven states of the event examples aren't scored yet; they need an input driver.
