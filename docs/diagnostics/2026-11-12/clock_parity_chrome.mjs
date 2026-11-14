// Clock parity: requestAnimationFrame's timestamp, Event.timeStamp, document.timeline.currentTime
// and performance.now() are one clock (DOMHighResTimeStamp, ms from the time origin) in the pinned
// Chrome (CfT 148). Prints the relations a benchmark relies on, as booleans and small numbers.
import puppeteer from "puppeteer-core";
import http from "http";
import fs from "fs";
const [exe, out] = process.argv.slice(2);
const PAGE_JS = fs.readFileSync(new URL('./clock_parity_page.js', import.meta.url), 'utf8');
const PAGE = `<!doctype html><meta charset=utf-8><body><button id=b>x</button><script>${PAGE_JS}</script>`;
const srv = http.createServer((q, s) => { s.setHeader("content-type", "text/html"); s.end(PAGE); }).listen(0, "127.0.0.1");
await new Promise(r => srv.on("listening", r));
const b = await puppeteer.launch({ executablePath: exe, headless: true, args: ["--no-sandbox", "--disable-gpu"] });
const p = await b.newPage();
await p.goto(`http://127.0.0.1:${srv.address().port}/`);
await p.waitForFunction("window.__out", { timeout: 15000 });
fs.writeFileSync(out, JSON.stringify(JSON.parse(await p.evaluate("window.__out")), null, 1));
console.log(await b.version());
await b.close(); srv.close();
