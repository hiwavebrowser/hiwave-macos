// LAYOUT-PER-FRAME, the Chrome arm: three runs of each arm of layout_per_frame_probe.html in Chrome for
// Testing, printed as JSON (layout_per_frame_probe_cft148.json): frames in 3000 ms and the
// median and maximum gap between frames.
//
// Usage, with this directory served on loopback (python3 -m http.server PORT --bind 127.0.0.1):
//   node layout_per_frame_probe_chrome.mjs <chrome-for-testing executable> [PORT, default 8766]
// puppeteer-core must be resolvable (npm i puppeteer-core in the working directory).
import puppeteer from "puppeteer-core";
const port = process.argv[3] || "8766";
const out = [];
for (const arm of ["tb", "lt", "data"]) {
  for (let i = 0; i < 3; i++) {
    const b = await puppeteer.launch({ executablePath: process.argv[2], headless: true, args: ["--no-sandbox", "--disable-gpu"] });
    const p = await b.newPage();
    await p.setViewport({ width: 1280, height: 720 });
    await p.goto(`http://127.0.0.1:${port}/layout_per_frame_probe.html#arm=${arm}`);
    await p.waitForFunction(() => window.rec && window.rec.done, { timeout: 60000, polling: 100 });
    const rec = await p.evaluate(() => window.rec);
    const gaps = rec.gaps.slice().sort((a, b) => a - b);
    out.push({ arm, frames: rec.frames, medianGap: gaps[gaps.length >> 1], maxGap: gaps[gaps.length - 1], version: await b.version() });
    await b.close();
  }
}
console.log(JSON.stringify(out, null, 1));
