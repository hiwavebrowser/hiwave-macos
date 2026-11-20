// IMPORT-SOURCE-PHASE, the Chrome arm: each module in import_source_phase_cases.json (with 'dep' and
// 'missing' made absolute, and a first import of a marker file) is imported from a blob: URL, printed
// as JSON {case: answer} (import_source_phase_cft148.json). Answers, as the Rust pin
// (crates/rustkit-js/tests/import_source_phase.rs) gives them: "ok:<export r>", "<ErrorName>" when the
// module parsed (its marker import was fetched) and then failed, "parse:<ErrorName>" when it did not.
//
// Usage, with this directory served on loopback (python3 -m http.server PORT --bind 127.0.0.1):
//   PARITY_CHROME_PATH=<pinned Chrome for Testing> node import_source_phase_chrome.mjs [PORT, default 8767]
// playwright-core must be resolvable (tools/parity_oracle's node_modules, or PLAYWRIGHT_CORE=<index.mjs>).
import { existsSync, readFileSync } from "node:fs";
const chrome = process.env.PARITY_CHROME_PATH;
if (!chrome) throw new Error("PARITY_CHROME_PATH is not set");
if (!existsSync(chrome)) throw new Error(`PARITY_CHROME_PATH set but not found: ${chrome}`);
const { chromium } = await import(process.env.PLAYWRIGHT_CORE || "playwright-core");
const port = process.argv[2] || "8767";
const cases = JSON.parse(readFileSync(new URL("./import_source_phase_cases.json", import.meta.url)));
const b = await chromium.launch({ executablePath: chrome, headless: true, args: ["--no-sandbox", "--disable-gpu", ...(process.env.V8_FLAGS ? [`--js-flags=${process.env.V8_FLAGS}`] : [])] });
const p = await b.newPage();
await p.goto(`http://127.0.0.1:${port}/import_source_phase_dep.js`);
const out = await p.evaluate(async ([cases, base]) => {
  const res = {};
  for (const [name, src0] of Object.entries(cases)) {
    const marker = `${base}/import_source_phase_side.js?${name}`;
    const src = `import '${marker}';\n` + src0
      .replaceAll("'dep'", `'${base}/import_source_phase_dep.js'`)
      .replaceAll("'missing'", `'${base}/import_source_phase_missing.js'`);
    const url = URL.createObjectURL(new Blob([src], { type: "text/javascript" }));
    try {
      const ns = await import(url);
      res[name] = "ok:" + String(ns.r);
    } catch (e) {
      const fetched = performance.getEntriesByName(marker).length > 0;
      res[name] = (fetched ? "" : "parse:") + (e && e.name);
    }
  }
  return res;
}, [cases, `http://127.0.0.1:${port}`]);
out._version = b.version();
await b.close();
console.log(JSON.stringify(out, null, 1));
