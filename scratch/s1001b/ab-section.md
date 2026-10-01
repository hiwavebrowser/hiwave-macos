Binaries: develop e4a82f7 vs fix fe23762. Machine load was 13 to 25 for the whole window (three lanes building), and that decided what could be measured.

**1. RustKit frames, A/B/A/B per site, no Chrome** (`parity-capture --url` at 1280x800; "within" is the same binary twice, so it is the site's own variance):

| site | within develop | within fix | develop vs fix |
|---|---|---|---|
| shopify | 0.00% | 0.00% | **4.48%** in all four pairings |
| google | 0.00% | 3.75% | 0.00% in two pairings, 3.75% in two |
| facebook | 0.00% | (one capture failed) | 0.00% |
| wikipedia | 0.00% | 0.00% | 0.00% |
| x | 0.00% | 0.00% | 0.00% |
| apple | 0.00% | 0.00% | 0.00% |
| lyft | 0.00% | 0.00% | 0.00% |
| github, netflix, squarespace | not measured: all four captures failed at load 25 | | |

- **shopify is the fix.** On develop the hero headline is Helvetica's glyphs squeezed onto the web font's advances ("Bethenext", letters touching). With the fix it is the site's own face with its own spacing, as in Chrome. Against the Chrome frame stored by the last quiet board (`20261001T0226Z-quiet-devaf4b95d`): **19.61% → 18.69%**. It does not cross the 15% line.
- **google's 3.75% is Google, not the fix**: one of the two fix captures got a different search-box variant; the other is pixel-identical to develop.
- Five sites are pixel-identical between the arms. I did not establish why their web fonts are unaffected; the likely reason is that a document with external stylesheets defers its first layout until they arrive, so there is no fallback first paint to poison the cache. Not verified.

**2. Board A/B with a fresh Chrome oracle: not usable tonight, and I am not claiming its number.** Eight sites ran in both arms before I stopped it (about 3 minutes per site per arm; all 20 would not fit the session). The pinned Chrome's screenshots timed out on several captures and RustKit loads failed under load in the develop arm (apple, shopify), so the totals (develop 8, fix 14 on those eight sites) are load noise: every site whose points differ has identical RustKit frames in both arms, or failed to load in one. Runs are kept on the hub branch under `trench/realsite/runs/20261001T0715Z-woff-ab-*`. A quiet full board after this lands is the real reading.

**Points: no change claimed.** Board stays 28/60 (22:48 quiet board, develop af4b95d).
