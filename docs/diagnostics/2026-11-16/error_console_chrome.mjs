// What reaches Chrome's console (the DevTools protocol's Runtime.exceptionThrown) for an uncaught error
// or an unhandled rejection the page cancelled, and for one it did not. Pinned Chrome (CfT 148), headless, no network.
// node error_console_chrome.mjs <chrome executable> <out.json>   (PW_FROM or cwd must resolve playwright-core)
import { createRequire } from "module";
import fs from "fs";
const require = createRequire(process.env.PW_FROM || import.meta.url);
const { chromium } = require("playwright-core");
const [exe, out] = process.argv.slice(2);
const cases = {
  "not cancelled": "",
  "window.onerror returns true": "window.onerror = function () { return true; };",
  "an error listener calls preventDefault": "window.addEventListener('error', function (ev) { ev.preventDefault(); });",
  "window.onerror returns false": "window.onerror = function () { return false; };",
};
const b = await chromium.launch({ executablePath: exe, headless: true, args: ["--disable-gpu"] });
const r = { browser: b.version(), console_exceptions: {} };
for (const [name, setup] of Object.entries(cases)) {
  const p = await b.newPage();
  const cdp = await p.context().newCDPSession(p);
  await cdp.send("Runtime.enable");
  let thrown = 0;
  cdp.on("Runtime.exceptionThrown", () => { thrown++; });
  await p.setContent(`<!doctype html><body><script>${setup}</script><script>setTimeout(function () { throw new Error("from a timer"); }, 0);</script><script>throw new Error("from a script");</script></body>`);
  await p.waitForTimeout(300);
  r.console_exceptions[name] = thrown;
  await p.close();
}
// The same for a promise rejected with no handler: `unhandledrejection` cancelled or not.
r.console_rejections = {};
for (const [name, setup] of Object.entries({
  "not cancelled": "",
  "an unhandledrejection listener calls preventDefault": "window.addEventListener('unhandledrejection', function (ev) { ev.preventDefault(); });",
  "window.onunhandledrejection returns false": "window.onunhandledrejection = function () { return false; };",
  "window.onunhandledrejection returns true": "window.onunhandledrejection = function () { return true; };",
})) {
  const p = await b.newPage();
  const cdp = await p.context().newCDPSession(p);
  await cdp.send("Runtime.enable");
  let thrown = 0;
  cdp.on("Runtime.exceptionThrown", () => { thrown++; });
  await p.setContent(`<!doctype html><body><script>${setup}</script><script>Promise.reject(new Error("rejected"));</script></body>`);
  await p.waitForTimeout(300);
  r.console_rejections[name] = thrown;
  await p.close();
}
fs.writeFileSync(out, JSON.stringify(r, null, 1));
console.log(JSON.stringify(r));
await b.close();
