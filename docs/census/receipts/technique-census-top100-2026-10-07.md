# Receipt — technique census top100 (proposed pin) 2026-10-07

> **Cleanup:** see `technique-census-top100-cleanup-2026-10-07.md` (hiwave-ops#3) — exclusive wall counts, 429 reclass, retry, proposed-pin banner on dated copy, single canonical JSON.

- Issue: #593
- Universe: `websuite/realsite-top100.json` (**PROPOSED PIN** — not BASELINE-dated)
- Method: keep prior top80 JSON (`docs/census/technique_census_2026-10-07.json`); measure only `websuite/realsite-top80-plus20.json` (20 new); merge + re-summarize
- Runner: `docs/census/tools/run_technique_census.mjs --list websuite/realsite-top80-plus20.json`
- Chromium: Playwright-bundled (same as top80 run)
- Handtest window: run at ~09:32–09:35 ET (outside xx:05 Z slot)

## Plus20 per-site log

```
OK       cargurus 11998ms http=200 js=120/5.53MB/>1MB=0 
OK       bringatrailer 15220ms http=200 js=52/7.11MB/>1MB=1 
OK       target 11476ms http=200 js=106/3.16MB/>1MB=0 
NOT RUN  bestbuy 30002ms http=- page.goto: Timeout 30000ms exceeded.
OK       costco 10086ms http=200 js=91/4.63MB/>1MB=0 
OK       homedepot 12756ms http=200 bw=akamai js=94/5.02MB/>1MB=0 
BOTWALL  etsy 3970ms http=403 bw=datadome js=3/0.54MB/>1MB=0 
OK       nike 11085ms http=200 bw=akamai js=117/6.99MB/>1MB=1 
OK       tiktok 9228ms http=200 js=113/2.96MB/>1MB=0 
OK       pinterest 9986ms http=200 js=78/2.93MB/>1MB=0 
OK       spotify 8521ms http=200 js=64/3MB/>1MB=1 
OK       twitch 10831ms http=200 js=148/2.45MB/>1MB=0 
BOTWALL  zillow 9836ms http=403 bw=perimeterx js=9/0.6MB/>1MB=0 
OK       expedia 5260ms http=429 js=14/0.32MB/>1MB=0 
NOT RUN  washingtonpost 239ms http=- page.goto: net::ERR_HTTP2_PROTOCOL_ERROR at https://www.washingtonpost.com/
BOTWALL  imdb 2215ms http=403 js=0/0MB/>1MB=0 
OK       duckduckgo 3323ms http=200 js=27/0.43MB/>1MB=0 
BOTWALL  adobe 4506ms http=403 bw=akamai js=0/0MB/>1MB=0 
OK       discord 12205ms http=200 bw=cloudflare js=27/9.61MB/>1MB=2 
OK       wayfair 10424ms http=429 bw=perimeterx+cloudflare js=10/0.73MB/>1MB=0 
wrote /Users/petecopeland/Repos/.worktrees/census/docs/census/technique_census_plus20_2026-10-07.json
```

## Outputs
- `docs/diagnostics/census-top100.md` (canonical publish for top100)
- `docs/diagnostics/census-top100.json`
- `docs/census/TECHNIQUE_CENSUS_TOP100_2026-10-07.md` (dated working copy)
- `docs/census/technique_census_top100_2026-10-07.json`
- `docs/census/technique_census_plus20_2026-10-07.json` (plus20 raw)
- Prior top80 docs left in place (`census-top80.md`)

## Ranking policy
See `websuite/realsite-top100.json` `ranking_policy` + `extension_notes`. Holdout20 fully subsumed by top80; cargurus+bringatrailer from top25; 18 additional US consumer vertical fills.
