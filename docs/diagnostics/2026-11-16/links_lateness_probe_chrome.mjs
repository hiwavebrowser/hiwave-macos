// LINKS-SLOW, the Chrome arm: five runs of links_lateness_probe.html in Chrome for Testing, printed as
// JSON (links_lateness_probe_cft148.json): frames, links and the links' lateness summary per run,
// and every link's lateness.
//
// Usage, with this directory served on loopback (python3 -m http.server PORT --bind 127.0.0.1):
//   node links_lateness_probe_chrome.mjs <chrome-for-testing executable> [PORT, default 8766]
import puppeteer from "puppeteer-core";
const port = process.argv[3] || "8766";
const runs = [];
for (let i = 0; i < 5; i++) {
  const b = await puppeteer.launch({ executablePath: process.argv[2], headless: true, args: ["--no-sandbox", "--disable-gpu"] });
  const p = await b.newPage();
  await p.goto(`http://127.0.0.1:${port}/links_lateness_probe.html`);
  await p.waitForFunction(() => window.rec && window.rec.done, { timeout: 30000, polling: 100 });
  runs.push(await p.evaluate(() => window.rec));
  runs[runs.length - 1].version = await b.version();
  await b.close();
}
console.log(JSON.stringify(runs, null, 1));
