// CLOCK-AT-PRESENT, the Chrome arm: five runs of clock_at_present_probe.html and five of
// clock_at_present_flood.html in Chrome for Testing, each to the page's rec.done, printed as
// JSON (clock_at_present_probe_cft148.json: { probe: [...], flood: [...] }).
//
// Usage, with this directory served on loopback (python3 -m http.server PORT --bind 127.0.0.1):
//   PARITY_CHROME_PATH=<pinned Chrome for Testing> node clock_at_present_probe_chrome.mjs [PORT, default 8766]
// Fails closed without PARITY_CHROME_PATH, as tools/parity_oracle/deterministic.mjs's
// getPinnedExecutablePath does after #1266 (switch to importing it once that lands).
// playwright-core must be resolvable from this file (tools/parity_oracle's node_modules: copy
// this file there, or set PLAYWRIGHT_CORE to its index.mjs).
import { existsSync } from "node:fs";
const chrome = process.env.PARITY_CHROME_PATH;
if (!chrome) throw new Error("PARITY_CHROME_PATH is not set: point it at the pinned Chrome for Testing (trench/BASELINE-<os>.md)");
if (!existsSync(chrome)) throw new Error(`PARITY_CHROME_PATH set but not found: ${chrome}`);
const { chromium } = await import(process.env.PLAYWRIGHT_CORE || "playwright-core");
const port = process.argv[2] || "8766";
const out = { probe: [], flood: [] };
for (const [page, list] of [["clock_at_present_probe.html", out.probe], ["clock_at_present_flood.html", out.flood]]) {
  for (let i = 0; i < 5; i++) {
    const b = await chromium.launch({ executablePath: chrome, headless: true, args: ["--no-sandbox", "--disable-gpu"] });
    const p = await b.newPage();
    await p.goto(`http://127.0.0.1:${port}/${page}`);
    await p.waitForFunction(() => window.rec && window.rec.done, null, { timeout: 30000, polling: 100 });
    const rec = await p.evaluate(() => window.rec);
    rec.version = b.version();
    list.push(rec);
    await b.close();
  }
}
console.log(JSON.stringify(out, null, 1));
