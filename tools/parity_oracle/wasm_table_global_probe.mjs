const det = await import(`${process.env.ORACLE_DIR}/deterministic.mjs`);
const { chromium } = await import(`${process.env.ORACLE_DIR}/node_modules/playwright/index.mjs`);
const browser = await chromium.launch(det.getDeterministicLaunchOptions({ requirePinned: true }));
const p = await browser.newPage(); await p.setContent('<!doctype html><body>w</body>');
const out = await p.evaluate(() => {
  const W = WebAssembly, r = {};
  const t = new W.Table({ element: 'anyfunc', initial: 2, maximum: 4 });
  r.len = t.length; r.get0 = t.get(0); r.tag = Object.prototype.toString.call(t);
  const g = new W.Global({ value: 'i32', mutable: true }, 41);
  r.g = g.value + '|' + g.valueOf() + '|' + Object.prototype.toString.call(g);
  const ex = new W.Instance(new W.Module(new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0, 1, 14, 3, 96, 0, 1, 127, 96, 1, 127, 1, 127, 96, 0, 1, 124, 2, 36, 3, 3, 101, 110, 118, 5, 116, 97, 98, 108, 101, 1, 112, 1, 2, 4, 3, 101, 110, 118, 1, 103, 3, 127, 1, 3, 101, 110, 118, 3, 110, 97, 110, 3, 124, 0, 3, 5, 4, 0, 1, 0, 2, 7, 33, 4, 5, 115, 101, 118, 101, 110, 0, 0, 8, 100, 105, 115, 112, 97, 116, 99, 104, 0, 1, 4, 103, 101, 116, 103, 0, 2, 3, 110, 97, 110, 0, 3, 9, 7, 1, 0, 65, 1, 11, 1, 0, 10, 24, 4, 4, 0, 65, 7, 11, 7, 0, 32, 0, 17, 0, 0, 11, 4, 0, 35, 0, 11, 4, 0, 35, 1, 11])), { env: { table: t, g: g, nan: NaN } }).exports;
  r.elem = t.get(1) === ex.seven; r.dispatch = ex.dispatch(1);
  try { ex.dispatch(0); } catch (e) { r.null_entry = e.constructor.name + ':' + e.message; }
  try { ex.dispatch(9); } catch (e) { r.oob = e.constructor.name + ':' + e.message; }
  t.set(0, ex.seven); r.after_set = ex.dispatch(0);
  r.getg = ex.getg(); g.value = 5; r.getg2 = ex.getg();
  r.nan = String(ex.nan());
  r.grow = t.grow(1) + '|' + t.length;
  try { t.grow(5); } catch (e) { r.grow_past = e.constructor.name; }
  try { t.set(0, () => 1); } catch (e) { r.set_js_fn = e.constructor.name; }
  try { t.get(99); } catch (e) { r.get_oob = e.constructor.name; }
  const ig = new W.Global({ value: 'i32' }, 3);
  try { ig.value = 4; } catch (e) { r.immutable_set = e.constructor.name; }
  r.i64 = typeof new W.Global({ value: 'i64' }, 5n).value;
  r.f32 = new W.Global({ value: 'f32' }, 1.1).value;
  try { new W.Instance(new W.Module(new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0, 1, 14, 3, 96, 0, 1, 127, 96, 1, 127, 1, 127, 96, 0, 1, 124, 2, 36, 3, 3, 101, 110, 118, 5, 116, 97, 98, 108, 101, 1, 112, 1, 2, 4, 3, 101, 110, 118, 1, 103, 3, 127, 1, 3, 101, 110, 118, 3, 110, 97, 110, 3, 124, 0, 3, 5, 4, 0, 1, 0, 2, 7, 33, 4, 5, 115, 101, 118, 101, 110, 0, 0, 8, 100, 105, 115, 112, 97, 116, 99, 104, 0, 1, 4, 103, 101, 116, 103, 0, 2, 3, 110, 97, 110, 0, 3, 9, 7, 1, 0, 65, 1, 11, 1, 0, 10, 24, 4, 4, 0, 65, 7, 11, 7, 0, 32, 0, 17, 0, 0, 11, 4, 0, 35, 0, 11, 4, 0, 35, 1, 11])), { env: { table: t, g: 41, nan: NaN } }); r.num_for_mut = 'ok'; } catch (e) { r.num_for_mut = e.constructor.name; }
  try { new W.Instance(new W.Module(new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0, 1, 14, 3, 96, 0, 1, 127, 96, 1, 127, 1, 127, 96, 0, 1, 124, 2, 36, 3, 3, 101, 110, 118, 5, 116, 97, 98, 108, 101, 1, 112, 1, 2, 4, 3, 101, 110, 118, 1, 103, 3, 127, 1, 3, 101, 110, 118, 3, 110, 97, 110, 3, 124, 0, 3, 5, 4, 0, 1, 0, 2, 7, 33, 4, 5, 115, 101, 118, 101, 110, 0, 0, 8, 100, 105, 115, 112, 97, 116, 99, 104, 0, 1, 4, 103, 101, 116, 103, 0, 2, 3, 110, 97, 110, 0, 3, 9, 7, 1, 0, 65, 1, 11, 1, 0, 10, 24, 4, 4, 0, 65, 7, 11, 7, 0, 32, 0, 17, 0, 0, 11, 4, 0, 35, 0, 11, 4, 0, 35, 1, 11])), { env: { table: t, g: g, nan: 'x' } }); r.str_for_f64 = 'ok'; } catch (e) { r.str_for_f64 = e.constructor.name; }
  try { new W.Instance(new W.Module(new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0, 1, 14, 3, 96, 0, 1, 127, 96, 1, 127, 1, 127, 96, 0, 1, 124, 2, 36, 3, 3, 101, 110, 118, 5, 116, 97, 98, 108, 101, 1, 112, 1, 2, 4, 3, 101, 110, 118, 1, 103, 3, 127, 1, 3, 101, 110, 118, 3, 110, 97, 110, 3, 124, 0, 3, 5, 4, 0, 1, 0, 2, 7, 33, 4, 5, 115, 101, 118, 101, 110, 0, 0, 8, 100, 105, 115, 112, 97, 116, 99, 104, 0, 1, 4, 103, 101, 116, 103, 0, 2, 3, 110, 97, 110, 0, 3, 9, 7, 1, 0, 65, 1, 11, 1, 0, 10, 24, 4, 4, 0, 65, 7, 11, 7, 0, 32, 0, 17, 0, 0, 11, 4, 0, 35, 0, 11, 4, 0, 35, 1, 11])), { env: { table: {}, g: g, nan: NaN } }); } catch (e) { r.bad_table = e.constructor.name; }
  r.table_externref = (() => { try { const x = new W.Table({ element: 'externref', initial: 1 }); x.set(0, {a: 1}); return typeof x.get(0); } catch (e) { return e.constructor.name; } })();
  return r;
});
console.log(JSON.stringify(out, null, 1)); await browser.close();
