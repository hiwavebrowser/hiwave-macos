// A module source nested too deep to parse, in the pinned Chrome (CfT 148): how the import settles.
// The same nestings as native_recursion_cases.json's parse_* cases, as the body of a module loaded with
// a dynamic import() of a blob: URL. Usage: node module_parse_depth_cases.mjs <chrome.exe> <cases.json> <results.json>
import puppeteer from "puppeteer-core";
import http from "http";
import fs from "fs";
const [exe, casesOut, resultsOut] = process.argv.slice(2);
const N = 200000;
// Each case builds the module source in the page (strings this long are not shipped as literals).
const cases = {
  module_nested_array_source: `'export default typeof ' + '['.repeat(${N}) + ']'.repeat(${N}) + ';'`,
  module_nested_parens: `'export default typeof ' + '('.repeat(${N}) + '1' + ')'.repeat(${N}) + ';'`,
  module_nested_blocks: `'{'.repeat(${N}) + '}'.repeat(${N}) + ' export default 1;'`,
  module_nested_functions: `'export default typeof ' + '(function(){'.repeat(${N}) + '})'.repeat(${N}) + ';'`,
  module_nested_template: `'export default typeof ' + '\`' + '\${\`'.repeat(${N}) + '\`}'.repeat(${N}) + '\`' + ';'`,
  module_shallow_ok: `'export default typeof ' + '['.repeat(10) + ']'.repeat(10) + ';'`,
};
fs.writeFileSync(casesOut, JSON.stringify(cases, null, 1));
const srv = http.createServer((q, r) => { r.setHeader("content-type", "text/html"); r.end("<!doctype html><body>"); }).listen(0, "127.0.0.1");
await new Promise(r => srv.on("listening", r));
const b = await puppeteer.launch({ executablePath: exe, headless: true, args: ["--no-sandbox", "--disable-gpu"] });
const p = await b.newPage();
await p.goto(`http://127.0.0.1:${srv.address().port}/`);
const res = {};
for (const [k, expr] of Object.entries(cases)) {
  res[k] = await p.evaluate(async (expr) => {
    const src = (0, eval)(expr);
    const url = URL.createObjectURL(new Blob([src], { type: "text/javascript" }));
    try { const m = await import(url); return "ok:" + String(m.default); }
    catch (e) { return "ERR:" + (e && e.name); }
  }, expr);
}
// The page still runs after the failed imports.
res.page_runs_on = await p.evaluate(() => "after:" + String([1, 2]));
fs.writeFileSync(resultsOut, JSON.stringify(res, null, 1));
console.log(await b.version());
await b.close(); srv.close();
