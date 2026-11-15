// Named access on document collections in the pinned Chrome (CfT 148), headless, local file only.
// node named_collections_chrome.mjs <chrome executable> <out.json>
// playwright-core is resolved from PW_FROM (a path inside a directory that has it, e.g.
// tools/parity_oracle/package.json after `npm ci`) or from this file's directory.
import { createRequire } from "module";
import fs from "fs";
import path from "path";
const require = createRequire(process.env.PW_FROM || import.meta.url);
const { chromium } = require("playwright-core");
const [exe, out] = process.argv.slice(2);
const dir = path.dirname(new URL(import.meta.url).pathname);
const b = await chromium.launch({ executablePath: exe, headless: true, args: ["--disable-gpu"] });
const p = await b.newPage();
await p.goto("file://" + path.join(dir, "named_collections.html"));
await p.waitForFunction("window.__out", null, { timeout: 15000 });
const r = { browser: b.version(), out: JSON.parse(await p.evaluate("window.__out")) };
fs.writeFileSync(out, JSON.stringify(r, null, 1) + "\n");
console.log(r.browser);
await b.close();
