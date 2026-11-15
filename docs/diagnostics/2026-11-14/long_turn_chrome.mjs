// What Chrome for Testing 148 does with one long turn, on the same page the engine runs
// (long_turn_engine.html: a requestAnimationFrame callback that runs 6 s after 4 s, a timer that runs 6 s
// at 12 s). The engine stops a turn after 5 s (its live-turn watchdog), and the stop ends a
// requestAnimationFrame loop, since the stopped callback never asks for its next frame; Chrome has no such
// stop. For the lead's question on the watchdog's default (MotionMark blocker 2). The page's own record is
// read after 22 s (it logs it at 21 s).
// Usage: node long_turn_chrome.mjs <chrome.exe> <long_turn_engine.html> <results.json>
import puppeteer from "puppeteer-core";
import fs from "fs";
const [exe, page, out] = process.argv.slice(2);
const b = await puppeteer.launch({ executablePath: exe, headless: true, args: ["--no-sandbox", "--disable-gpu"] });
const p = await b.newPage();
await p.setContent(fs.readFileSync(page, "utf8"));
await new Promise((r) => setTimeout(r, 22000));
const rec = await p.evaluate(() => window.rec);
fs.writeFileSync(out, JSON.stringify({ chrome: await b.version(), page: "long_turn_engine.html", ...rec }, null, 1));
console.log(JSON.stringify(rec));
await b.close();
