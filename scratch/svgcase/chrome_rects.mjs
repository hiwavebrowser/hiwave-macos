// usage: node chrome_rects.mjs <html>  -> id: x y w h for every element with an id (pinned CfT 148)
import { chromium } from '/Users/petecopeland/Repos/.worktrees/trench-realsite/tools/parity_oracle/node_modules/playwright/index.mjs';
import { resolve } from 'path';
const exe = `${process.env.HOME}/Repos/hiwave/hiwave-macos/.browsers/chrome/mac_arm-148.0.7778.216/chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing`;
const b = await chromium.launch({ executablePath: exe, headless: true });
const p = await b.newPage({ viewport: { width: 400, height: 1000 } });
await p.goto('file://' + resolve(process.argv[2]));
const r = await p.evaluate(() => [...document.querySelectorAll('[id]')].map(e => {
  const q = e.getBoundingClientRect(); return `${e.id}: ${q.x} ${q.y} ${q.width} ${q.height}`; }));
console.log(r.join('\n'));
await b.close();
