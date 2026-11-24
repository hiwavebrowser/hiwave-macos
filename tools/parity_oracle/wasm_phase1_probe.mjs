const det = await import(`${process.env.ORACLE_DIR}/deterministic.mjs`);
const { chromium } = await import(`${process.env.ORACLE_DIR}/node_modules/playwright/index.mjs`);
const browser = await chromium.launch(det.getDeterministicLaunchOptions({ requirePinned: true }));
const p = await browser.newPage();
await p.setContent('<!doctype html><body>w</body>');
const out = await p.evaluate(async () => {
  // (module (func (export "add") (param i32 i32) (result i32) local.get 0 local.get 1 i32.add))
  const add = new Uint8Array([0,97,115,109,1,0,0,0,1,7,1,96,2,127,127,1,127,3,2,1,0,7,7,1,3,97,100,100,0,0,10,9,1,7,0,32,0,32,1,106,11]);
  // (module (func (export "big") (result i64) i64.const -1))
  const big = new Uint8Array([0,97,115,109,1,0,0,0,1,5,1,96,0,1,126,3,2,1,0,7,7,1,3,98,105,103,0,0,10,6,1,4,0,66,127,11]);
  // (module (import "env" "f" (func (param i32) (result i32))) (func (export "g") (param i32) (result i32) local.get 0 call 0))
  const imp = new Uint8Array([0,97,115,109,1,0,0,0,1,6,1,96,1,127,1,127,2,9,1,3,101,110,118,1,102,0,0,3,2,1,0,7,5,1,1,103,0,1,10,8,1,6,0,32,0,16,0,11]);
  // (module (func (export "t") unreachable))
  const trap = new Uint8Array([0,97,115,109,1,0,0,0,1,4,1,96,0,0,3,2,1,0,7,5,1,1,116,0,0,10,5,1,3,0,0,11]);
  const r = {};
  const W = WebAssembly;
  r.typeof = typeof W; r.toString = Object.prototype.toString.call(W);
  r.ctor_len = ['Module','Instance','Memory','Table','Global'].map(n => n + ':' + W[n].length).join(',');
  r.validate_good = W.validate(add); r.validate_bad = W.validate(new Uint8Array([0,97,115,109,2,0,0,0]));
  try { new W.Module(new Uint8Array([1,2,3])); } catch (e) { r.bad_module = e.constructor.name + '|' + (e instanceof W.CompileError) + '|' + e.message; }
  try { W.Module(add); } catch (e) { r.module_no_new = e.constructor.name + '|' + e.message; }
  const m = new W.Module(add);
  r.module_exports = JSON.stringify(W.Module.exports(m)); r.module_imports = JSON.stringify(W.Module.imports(new W.Module(imp)));
  const inst = new W.Instance(m);
  const ex = inst.exports;
  r.exports_proto = Object.getPrototypeOf(ex) === null; r.exports_frozen = Object.isFrozen(ex);
  r.add = ex.add(2, 3) + '|' + ex.add(0x7fffffff, 1) + '|' + ex.add('4', 1.9);
  r.fn_name_len = ex.add.name + '|' + ex.add.length + '|' + typeof ex.add;
  r.big = typeof new W.Instance(new W.Module(big)).exports.big() + '|' + String(new W.Instance(new W.Module(big)).exports.big());
  const gi = new W.Instance(new W.Module(imp), { env: { f: x => x * 10 } }).exports.g;
  r.import_call = gi(7);
  try { new W.Instance(new W.Module(imp), { env: { f: () => { throw new TypeError('from js'); } } }).exports.g(1); } catch (e) { r.import_throw = e.constructor.name + '|' + e.message; }
  try { new W.Instance(new W.Module(imp), {}); } catch (e) { r.missing_import = e.constructor.name + '|' + e.message; }
  try { new W.Instance(new W.Module(imp), { env: { f: 5 } }); } catch (e) { r.bad_import = e.constructor.name + '|' + e.message; }
  try { new W.Instance(new W.Module(trap)).exports.t(); } catch (e) { r.trap = e.constructor.name + '|' + (e instanceof W.RuntimeError) + '|' + e.message; }
  r.errors = ['CompileError','LinkError','RuntimeError'].map(n => n + ':' + (W[n].prototype instanceof Error) + ':' + W[n].name).join(',');
  const res = await W.instantiate(add); r.instantiate_bytes = Object.keys(res).join(',') + '|' + res.instance.exports.add(1,1);
  const res2 = await W.instantiate(m); r.instantiate_module = Object.prototype.toString.call(res2) + '|' + (res2 instanceof W.Instance);
  r.compile = (await W.compile(add)) instanceof W.Module;
  return r;
});
console.log(JSON.stringify(out, null, 1));
await browser.close();
