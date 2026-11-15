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
  const text = await p.$eval("#out", (e) => e.textContent);
  const cells = {};
  for (const part of text.split(";;")) {
    const [id, size] = part.split("=");
    const [w, h] = size.split("x");
    cells[id] = [parseFloat(w), parseFloat(h)];
  }
  fs.writeFileSync(out, JSON.stringify(cells, null, 1));
  console.log(Object.keys(cells).length, "cells; browser", await browser.version());
} finally {
  await browser.close();
}
