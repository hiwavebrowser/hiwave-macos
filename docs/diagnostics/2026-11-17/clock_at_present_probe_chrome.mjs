// CLOCK-AT-PRESENT, the Chrome arm: five runs of clock_at_present_probe.html in Chrome for
// Testing, each to the page's rec.done, printed as JSON (clock_at_present_probe_cft148.json).
//
// Usage, with this directory served on loopback (python3 -m http.server PORT --bind 127.0.0.1):
//   node clock_at_present_probe_chrome.mjs <chrome-for-testing executable> [PORT, default 8766]
// playwright-core must be resolvable from this file (tools/parity_oracle's node_modules: copy
// this file there, or set PLAYWRIGHT_CORE to its index.mjs).
const { chromium } = await import(process.env.PLAYWRIGHT_CORE || "playwright-core");
const port = process.argv[3] || "8766";
const runs = [];
for (let i = 0; i < 5; i++) {
  const b = await chromium.launch({ executablePath: process.argv[2], headless: true, args: ["--no-sandbox", "--disable-gpu"] });
  const p = await b.newPage();
  await p.goto(`http://127.0.0.1:${port}/clock_at_present_probe.html`);
  await p.waitForFunction(() => window.rec && window.rec.done, null, { timeout: 30000, polling: 100 });
  const rec = await p.evaluate(() => window.rec);
  rec.version = b.version();
  runs.push(rec);
  await b.close();
}
console.log(JSON.stringify(runs, null, 1));
