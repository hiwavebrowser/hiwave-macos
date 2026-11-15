// Border-box sizes of the elements with an id in a page, on whatever Chrome is given.
// usage: node cb3c_rects_chrome.mjs <chrome.exe> <page.html> <out.json>   (needs puppeteer-core)
import puppeteer from "puppeteer-core";
import fs from "fs";
import { pathToFileURL } from "url";

const [exe, page, out] = process.argv.slice(2);
const browser = await puppeteer.launch({
  executablePath: exe,
  headless: true,
  args: ["--no-sandbox", "--disable-gpu", "--window-size=800,600"],
  defaultViewport: { width: 800, height: 600 },
});
try {
  const p = await browser.newPage();
  await p.goto(pathToFileURL(page).href);
  const rects = await p.evaluate(() => {
    const o = {};
    for (const e of document.querySelectorAll("[id]")) {
      const r = e.getBoundingClientRect();
      o[e.id] = [r.width, r.height];
    }
    return o;
  });
  fs.writeFileSync(out, JSON.stringify(rects, null, 1));
  console.log(Object.keys(rects).length, "elements; browser", await browser.version());
} finally {
  await browser.close();
}
