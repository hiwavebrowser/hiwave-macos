// PACE1, the Chrome arm: three runs of long_turn_resume.html in Chrome for Testing, each to the page's
// rec.done, printed as JSON (long_turn_resume_cft148.json).
//
// Usage, with this directory served on loopback (python3 -m http.server PORT --bind 127.0.0.1):
//   node long_turn_resume_chrome.mjs <chrome-for-testing executable> [PORT, default 8766]
// puppeteer-core must be resolvable (npm i puppeteer-core in the working directory).
import puppeteer from "puppeteer-core";
const port = process.argv[3] || "8766";
const runs = [];
for (let i = 0; i < 3; i++) {
  const b = await puppeteer.launch({ executablePath: process.argv[2], headless: true, args: ["--no-sandbox", "--disable-gpu"] });
  const p = await b.newPage();
  await p.goto(`http://127.0.0.1:${port}/long_turn_resume.html`);
  await p.waitForFunction(() => window.rec && window.rec.done, { timeout: 15000, polling: 100 });
  runs.push(await p.evaluate(() => window.rec));
  runs[runs.length - 1].version = await b.version();
  await b.close();
}
console.log(JSON.stringify(runs, null, 1));
