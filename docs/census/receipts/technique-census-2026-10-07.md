# Technique census receipt — 2026-10-07

- **Branch**: `census/technique-census-2026-10-07`
- **Worktree**: `/Users/petecopeland/Repos/.worktrees/census`
- **Chrome**: Playwright-bundled Chromium 143.0.7499.4 (`playwright-bundled:chromium-1200`)
  - Path: `~/Library/Caches/ms-playwright/chromium-1200/chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing`
  - `PARITY_CHROME_PATH` unset; headless
- **Start (ET)**: Tue Oct 6, 2026 8:47:17 PM EDT
- **End (ET)**: Tue Oct 6, 2026 8:56:38 PM EDT (~9.4 min)
- **Universe**: `websuite/realsite-top80.json` (80 sites)
- **Results**: OK 57 / BOTWALL(403) 22 / FAIL 1 (`x` net::ERR_HTTP_RESPONSE_CODE_FAILURE)
- **Known bot walls confirmed**: chrono24 Cloudflare 403, cars.com Cloudflare 403
- **Outputs**:
  - Canonical: `docs/diagnostics/census-top80.md` + `docs/diagnostics/census-top80.json`
  - Dated: `docs/census/TECHNIQUE_CENSUS_2026-10-07.md` + `docs/census/technique_census_2026-10-07.json`
- **Tools**: `docs/census/tools/{run_technique_census.mjs,summarize_technique_census.mjs,technique_classifier.js}`
- **Handtest**: No Handtest GUI in flight during run; finished before ~9:05pm ET Z slot
- **Commit / PR**: filled after git push

## Git
- **SHA**: `296bfe2942919171416e5b41211cacbd9bdf2e7b` (results commit; scaffold was `131d20b8` after rebase onto `50e77c83`)
- **PR**: https://github.com/hiwavebrowser/hiwave-macos/pull/572
- **Compare**: https://github.com/hiwavebrowser/hiwave-macos/compare/develop...census/technique-census-2026-10-07
