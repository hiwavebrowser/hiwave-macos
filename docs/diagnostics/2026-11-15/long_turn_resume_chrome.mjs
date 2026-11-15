import puppeteer from "puppeteer-core";
const runs = [];
for (let i = 0; i < 3; i++) {
  const b = await puppeteer.launch({ executablePath: process.argv[2], headless: true, args: ["--no-sandbox", "--disable-gpu"] });
  const p = await b.newPage();
  await p.goto("http://127.0.0.1:8766/long_turn_resume.html");
  await p.waitForFunction(() => window.rec && window.rec.done, { timeout: 15000, polling: 100 });
  runs.push(await p.evaluate(() => window.rec));
  runs[runs.length - 1].version = await b.version();
  await b.close();
}
console.log(JSON.stringify(runs, null, 1));
