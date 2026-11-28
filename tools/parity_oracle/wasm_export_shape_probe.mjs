import { chromium } from '/home/petec/repos/hiwave-linux-3way2/tools/parity_oracle/node_modules/playwright/index.mjs';
const b = await chromium.launch({ executablePath: process.env.PARITY_CHROME_PATH, headless: true });
const p = await b.newPage();
const v = await p.evaluate(() => {
  const bytes = new Uint8Array([0,97,115,109,1,0,0,0,1,6,1,96,1,127,1,127,3,2,1,0,7,5,1,1,102,0,0,10,6,1,4,0,32,0,11]);
  const f = new WebAssembly.Instance(new WebAssembly.Module(bytes)).exports.f;
  const r = ['prototype' in f, Object.getPrototypeOf(f) === Function.prototype, f.name, f.length,
    JSON.stringify(Object.getOwnPropertyNames(f).sort())];
  try { new f(1); r.push('constructed'); } catch (e) { r.push(e.constructor.name); }
  r.push(f.call(null, 5), f.apply(null, [6]), f(7, 8, 9), f());
  return r.join('|');
});
console.log(JSON.stringify({ chrome: await b.version(), result: v }));
await b.close();
