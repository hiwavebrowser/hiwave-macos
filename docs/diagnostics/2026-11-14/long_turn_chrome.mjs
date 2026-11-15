// What Chrome for Testing 148 does with one long turn: a requestAnimationFrame callback that runs 6 s,
// and a timer that runs 6 s. The engine stops a turn after 5 s (its live-turn watchdog), and the stop
// ends a requestAnimationFrame loop, since the stopped callback never asks for its next frame; Chrome has no
// such stop. For the lead's question on the watchdog's default (MotionMark blocker 2).
// Usage: node long_turn_chrome.mjs <chrome.exe> <results.json>
import puppeteer from "puppeteer-core";
import fs from "fs";
const [exe, out] = process.argv.slice(2);
const b = await puppeteer.launch({ executablePath: exe, headless: true, args: ["--no-sandbox", "--disable-gpu"] });
const p = await b.newPage();
await p.setContent(`<!doctype html><script>
window.rec = { frames: 0, framesAfterLong: 0, longFrameMs: 0, timerLongMs: 0, timersAfter: 0, error: null };
var n = 0, longDone = false;
function frame(ts) {
  n++; rec.frames = n;
  if (longDone) rec.framesAfterLong++;
  if (n === 10) { var t0 = performance.now(); while (performance.now() - t0 < 6000) {} rec.longFrameMs = Math.round(performance.now() - t0); longDone = true; }
  requestAnimationFrame(frame);
}
requestAnimationFrame(frame);
setTimeout(function () { var t0 = performance.now(); while (performance.now() - t0 < 6000) {} rec.timerLongMs = Math.round(performance.now() - t0);
  setTimeout(function tick() { rec.timersAfter++; if (rec.timersAfter < 5) setTimeout(tick, 50); }, 50); }, 100);
</script>`);
await new Promise((r) => setTimeout(r, 16000));
const rec = await p.evaluate(() => window.rec);
fs.writeFileSync(out, JSON.stringify({ chrome: await b.version(), ...rec }, null, 1));
console.log(JSON.stringify(rec));
await b.close();
