// STYLE1b (a), the Chrome arm: style1b_contract_page.js in Chrome for Testing, three runs, each
// printed as JSON (style1b_contract_cft148.json). The runs must agree; the first is the contract.
//
// Usage, with this directory served on loopback (python3 -m http.server PORT --bind 127.0.0.1):
//   node style1b_contract_chrome.mjs <chrome-for-testing executable> [PORT, default 8766]
// puppeteer-core must be resolvable (npm i puppeteer-core in the working directory).
import puppeteer from "puppeteer-core";
const port = process.argv[3] || "8766";
const runs = [];
for (let i = 0; i < 3; i++) {
  const b = await puppeteer.launch({ executablePath: process.argv[2], headless: true, args: ["--no-sandbox", "--disable-gpu"] });
  const p = await b.newPage();
  await p.goto(`http://127.0.0.1:${port}/style1b_contract.html`);
  const out = await p.evaluate(() => STYLE1B_RUN());
  runs.push({ version: await b.version(), cases: out });
  await b.close();
}
const same = runs.every((r) => JSON.stringify(r.cases) === JSON.stringify(runs[0].cases));
console.log(JSON.stringify({ version: runs[0].version, runsAgree: same, cases: runs[0].cases }, null, 1));
