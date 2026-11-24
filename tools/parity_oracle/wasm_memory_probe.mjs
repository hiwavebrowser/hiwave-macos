const det = await import(`${process.env.ORACLE_DIR}/deterministic.mjs`);
const { chromium } = await import(`${process.env.ORACLE_DIR}/node_modules/playwright/index.mjs`);
const browser = await chromium.launch(det.getDeterministicLaunchOptions({ requirePinned: true }));
const p = await browser.newPage(); await p.setContent('<!doctype html><body>w</body>');
const out = await p.evaluate(() => {
  const W = WebAssembly, r = {};
  const m = new W.Memory({ initial: 1, maximum: 2 });
  r.byteLength = m.buffer.byteLength; r.same = m.buffer === m.buffer; r.ctor = m.buffer.constructor.name;
  r.tag = Object.prototype.toString.call(m);
  try { m.buffer.transfer(); r.transfer = 'ok'; } catch (e) { r.transfer = e.constructor.name; }
  try { structuredClone(m.buffer, { transfer: [m.buffer] }); r.sc = 'ok'; } catch (e) { r.sc = e.constructor.name; }
  const ex = new W.Instance(new W.Module(new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0, 1, 15, 3, 96, 1, 127, 1, 127, 96, 2, 127, 127, 0, 96, 0, 1, 127, 2, 16, 1, 3, 101, 110, 118, 6, 109, 101, 109, 111, 114, 121, 2, 1, 1, 2, 3, 5, 4, 0, 1, 2, 0, 7, 30, 4, 4, 108, 111, 97, 100, 0, 0, 5, 115, 116, 111, 114, 101, 0, 1, 4, 115, 105, 122, 101, 0, 2, 4, 103, 114, 111, 119, 0, 3, 10, 31, 4, 7, 0, 32, 0, 40, 2, 0, 11, 9, 0, 32, 0, 32, 1, 54, 2, 0, 11, 4, 0, 63, 0, 11, 6, 0, 32, 0, 64, 0, 11])), { env: { memory: m } }).exports;
  new Int32Array(m.buffer)[4] = 1234; r.wasm_reads_js = ex.load(16);
  ex.store(32, -7); r.js_reads_wasm = new Int32Array(m.buffer)[8];
  r.size = ex.size(); r.grow0 = m.grow(0); r.same_after_grow0 = m.buffer.byteLength;
  const before = m.buffer; r.grow1 = m.grow(1); r.detached_old = before.byteLength; r.new_len = m.buffer.byteLength; r.size_after = ex.size();
  try { m.grow(1); r.grow_past_max = 'ok'; } catch (e) { r.grow_past_max = e.constructor.name; }
  r.wasm_grow_past_max = ex.grow(1);
  try { new W.Memory({}); } catch (e) { r.no_initial = e.constructor.name; }
  try { new W.Memory({ initial: 2, maximum: 1 }); } catch (e) { r.min_gt_max = e.constructor.name; }
  try { new W.Instance(new W.Module(new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0, 1, 15, 3, 96, 1, 127, 1, 127, 96, 2, 127, 127, 0, 96, 0, 1, 127, 2, 16, 1, 3, 101, 110, 118, 6, 109, 101, 109, 111, 114, 121, 2, 1, 1, 2, 3, 5, 4, 0, 1, 2, 0, 7, 30, 4, 4, 108, 111, 97, 100, 0, 0, 5, 115, 116, 111, 114, 101, 0, 1, 4, 115, 105, 122, 101, 0, 2, 4, 103, 114, 111, 119, 0, 3, 10, 31, 4, 7, 0, 32, 0, 40, 2, 0, 11, 9, 0, 32, 0, 32, 1, 54, 2, 0, 11, 4, 0, 63, 0, 11, 6, 0, 32, 0, 64, 0, 11])), { env: { memory: {} } }); } catch (e) { r.bad_mem_import = e.constructor.name; }
  try { new W.Instance(new W.Module(new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0, 1, 15, 3, 96, 1, 127, 1, 127, 96, 2, 127, 127, 0, 96, 0, 1, 127, 2, 16, 1, 3, 101, 110, 118, 6, 109, 101, 109, 111, 114, 121, 2, 1, 1, 2, 3, 5, 4, 0, 1, 2, 0, 7, 30, 4, 4, 108, 111, 97, 100, 0, 0, 5, 115, 116, 111, 114, 101, 0, 1, 4, 115, 105, 122, 101, 0, 2, 4, 103, 114, 111, 119, 0, 3, 10, 31, 4, 7, 0, 32, 0, 40, 2, 0, 11, 9, 0, 32, 0, 32, 1, 54, 2, 0, 11, 4, 0, 63, 0, 11, 6, 0, 32, 0, 64, 0, 11])), { env: { memory: new W.Memory({ initial: 1, maximum: 3 }) } }); r.max_too_big = 'ok'; } catch (e) { r.max_too_big = e.constructor.name; }
  const own = new W.Instance(new W.Module(new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0, 1, 5, 1, 96, 0, 1, 127, 3, 2, 1, 0, 5, 3, 1, 0, 1, 7, 14, 2, 4, 115, 105, 122, 101, 0, 0, 3, 109, 101, 109, 2, 0, 10, 6, 1, 4, 0, 63, 0, 11]))).exports;
  r.own_mem = Object.prototype.toString.call(own.mem) + '|' + own.mem.buffer.byteLength + '|' + own.size();
  r.m1 = new W.Memory({ initial: 0 }).buffer.byteLength;
  return r;
});
console.log(JSON.stringify(out, null, 1)); await browser.close();
