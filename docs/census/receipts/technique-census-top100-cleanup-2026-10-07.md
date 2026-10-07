# Receipt — technique census top100 cleanup (after #597) 2026-10-07

- Private board: petec4244/hiwave-ops#3
- Follows: hiwave-macos #597 (merge `8a12d560`)
- Universe: `websuite/realsite-top100.json` (**PROPOSED PIN** — unchanged; no pin swaps)

## What changed
1. **Exclusive status** — Loaded OK / BOTWALL / NOT RUN no longer overlap. Runner treats HTTP 401/403/429/503 and challenge-title pages as `BOTWALL`.
2. **Reclassified from existing JSON** (no re-burn): `glassdoor` (401, Cloudflare “Just a moment…”) and `weather` (429, Vercel Security Checkpoint).
3. **Light re-probe** (with one goto retry): `expedia`, `wayfair`, `bestbuy`, `washingtonpost`.
4. **Proposed-pin banner** mirrored onto dated `docs/census/TECHNIQUE_CENSUS_TOP100_2026-10-07.md`.
5. **Single canonical sidecar** — keep `docs/diagnostics/census-top100.json`; removed duplicate `docs/census/technique_census_top100_2026-10-07.json`.

## Re-probe log
See `docs/census/receipts/run_cleanup_reprobe4_2026-10-07.log` and `docs/census/technique_census_cleanup_reprobe4_2026-10-07.json`.

```
NOT RUN  bestbuy (timeout ×2)
BOTWALL  expedia http=429 title≈Bot or Not?
NOT RUN  washingtonpost (ERR_HTTP2_PROTOCOL_ERROR ×2)
BOTWALL  wayfair http=429 bw=perimeterx+cloudflare title≈Access denied
```

## Count delta (exclusive)

| | Before (#597) | After cleanup |
|---|---:|---:|
| OK | 71 | **67** |
| BOTWALL | 26 (md said 27 w/ glassdoor double-count) | **30** |
| NOT RUN | 3 | **3** |
| Sum | 100 (md headline summed 101) | **100** |

Moved OK→BOTWALL: glassdoor, weather, expedia, wayfair. Fails unchanged after retry: bestbuy, washingtonpost (+ prior top80 fail still present).

## Outputs
- Canonical: `docs/diagnostics/census-top100.md` + `.json`
- Dated working: `docs/census/TECHNIQUE_CENSUS_TOP100_2026-10-07.md` (banner + tables; JSON twin removed)
- Tools: runner retry + wall classification; summarizer exclusive buckets + banner
- Docs/scripts only — no engine code

## Full re-probe console

```
NOT RUN  bestbuy 30002ms http=- attempt=1 page.goto: Timeout 30000ms exceeded.
RETRY    bestbuy after fail: page.goto: Timeout 30000ms exceeded.
NOT RUN  bestbuy 30000ms http=- attempt=2 page.goto: Timeout 30000ms exceeded.
BOTWALL  expedia 4355ms http=429 js=14/0.32MB/>1MB=0 attempt=1 
NOT RUN  washingtonpost 229ms http=- attempt=1 page.goto: net::ERR_HTTP2_PROTOCOL_ERROR at https://www.washingtonpost.com/
RETRY    washingtonpost after fail: page.goto: net::ERR_HTTP2_PROTOCOL_ERROR at https://www.washingtonpost.com/
NOT RUN  washingtonpost 211ms http=- attempt=2 page.goto: net::ERR_HTTP2_PROTOCOL_ERROR at https://www.washingtonpost.com/
BOTWALL  wayfair 10319ms http=429 bw=perimeterx+cloudflare js=10/0.88MB/>1MB=0 attempt=1 
wrote /Users/petecopeland/Repos/.worktrees/census/docs/census/technique_census_cleanup_reprobe4_2026-10-07.json
```
