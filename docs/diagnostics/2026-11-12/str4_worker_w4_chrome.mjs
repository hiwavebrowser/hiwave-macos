// STR-4 W4: structured clone through postMessage, page -> worker -> page, in the pinned
// Chrome (CfT 148). A value is described by `desc` (types, own data, identity) so the log is JSON.
import puppeteer from "puppeteer-core";
import http from "http";
import fs from "fs";
const [exe, casesOut, out] = process.argv.slice(2);
const DESC = `
function desc(v, seen) {
  seen = seen || [];
  var t = typeof v;
  if (v === undefined) return 'undefined';
  if (v === null) return 'null';
  if (t === 'number') return Object.is(v, -0) ? '-0' : Number.isNaN(v) ? 'NaN' : 'n:' + v;
  if (t === 'bigint') return 'big:' + v;
  if (t === 'string' || t === 'boolean') return t + ':' + v;
  var i = seen.indexOf(v); if (i >= 0) return 'ref#' + i;
  seen.push(v);
  var tag = Object.prototype.toString.call(v).slice(8, -1);
  if (v instanceof Date) return 'Date:' + v.getTime();
  if (v instanceof RegExp) return 'RegExp:' + v.source + '/' + v.flags + ':' + v.lastIndex;
  if (v instanceof Map) return { Map: Array.from(v.entries()).map(function (e) { return [desc(e[0], seen), desc(e[1], seen)]; }) };
  if (v instanceof Set) return { Set: Array.from(v.values()).map(function (e) { return desc(e, seen); }) };
  if (v instanceof ArrayBuffer) return 'AB:' + v.byteLength + ':' + Array.from(new Uint8Array(v)).join(',');
  if (ArrayBuffer.isView(v)) return tag + ':' + Array.from(v).join(',') + ':off' + v.byteOffset + ':len' + v.length + ':buf' + v.buffer.byteLength;
  if (v instanceof Error) return { Error: [v.name, v.message, v.constructor === Error || v.constructor === TypeError || v.constructor === RangeError] };
  if (v instanceof Boolean || v instanceof Number || v instanceof String) return 'Wrapper:' + tag + ':' + String(v.valueOf());
  if (Array.isArray(v)) { var a = []; for (var k = 0; k < v.length; k++) a.push(k in v ? desc(v[k], seen) : 'HOLE'); var ex = Object.keys(v).filter(function (x) { return String(x >>> 0) !== x; }); return { Array: a, extra: ex.map(function (x) { return [x, desc(v[x], seen)]; }) }; }
  var o = { tag: tag, proto: Object.getPrototypeOf(v) === Object.prototype ? 'Object' : Object.getPrototypeOf(v) === null ? 'null' : 'other', keys: {} };
  Object.keys(v).forEach(function (x) { o.keys[x] = desc(v[x], seen); });
  return o;
}`;
const CASES = {
  prims: "[undefined, null, true, 'str', 1.5, -0, NaN, Infinity, 10n, '']",
  object: "({ a: 1, b: { c: [1, 2, 3] }, u: undefined, n: null })",
  sparse_array: "(function(){ var a = [1,,3]; a.extra = 'x'; return a; })()",
  date: "new Date(1234567890123)",
  invalid_date: "new Date(NaN)",
  regexp: "(function(){ var r = /a(b)*c/gi; r.lastIndex = 3; return r; })()",
  map_set: "new Map([[1, { a: 1 }], ['k', new Set([1, 2])]])",
  typed_arrays: "[new Uint8Array([1,2,3]), new Float64Array([1.5, -0, NaN]), new Int16Array(new ArrayBuffer(8), 2, 2)]",
  arraybuffer: "new Uint8Array([9,8,7]).buffer",
  shared_view_buffer: "(function(){ var b = new ArrayBuffer(8); return { x: new Uint8Array(b), y: new Uint16Array(b) }; })()",
  dataview: "new DataView(new ArrayBuffer(4), 1, 2)",
  error: "new RangeError('boom')",
  error_plain: "new Error('plain')",
  wrappers: "[new Boolean(false), new Number(5), new String('s')]",
  cycle: "(function(){ var o = { name: 'o' }; o.self = o; o.list = [o, { back: o }]; return o; })()",
  shared_ref: "(function(){ var s = { v: 1 }; return [s, s]; })()",
  class_instance: "(function(){ class P { constructor(){ this.x = 1; } get y(){ return 2; } m(){} } return new P(); })()",
  proto_null: "Object.assign(Object.create(null), { a: 1 })",
  symbol_key_ignored: "(function(){ var o = { a: 1 }; o[Symbol('s')] = 2; return o; })()",
  getter_invoked: "({ get g(){ return 'got'; } })",
  non_enumerable_dropped: "(function(){ var o = { a: 1 }; Object.defineProperty(o, 'hidden', { value: 2 }); return o; })()",
  nested_map_cycle: "(function(){ var m = new Map(); m.set('me', m); return m; })()",
  deep: "(function(){ var o = {}; var c = o; for (var i = 0; i < 30; i++) { c.n = {}; c = c.n; } return o; })()",
};
const THROWERS = {
  function: "(function(){})",
  symbol: "Symbol('x')",
  weak: "new WeakMap()",
  promise: "Promise.resolve(1)",
  object_with_function: "({ f: function(){} })",
  proxy: "new Proxy({}, {})",
  dom_node: "document.body",
};
const WORKER = DESC + `
onmessage = function (e) {
  var d = e.data;
  if (d && d.__k === 'echo') postMessage({ k: d.k, back: d.v, seenByWorker: desc(d.v) });
  else if (d && d.__k === 'transfer') {
    var before = d.v.byteLength; postMessage({ k: 'transferred', size: before, got: desc(d.v) });
    postMessage({ k: 'reply-transfer', buf: d.v }, [d.v]);
    postMessage({ k: 'after-worker-send', len: d.v.byteLength });
  }
};`;
const PAGE_JS = `
${DESC}
var res = {}, w = new Worker('/w.js');
var CASES = ${JSON.stringify(CASES)}, THROWERS = ${JSON.stringify(THROWERS)};
w.onmessage = function (e) {
  var d = e.data;
  if (d.k === 'transferred') { res.transfer_worker_saw = [d.size, d.got]; return; }
  if (d.k === 'reply-transfer') { res.transfer_back = desc(d.buf); return; }
  if (d.k === 'after-worker-send') { res.worker_len_after_send = d.len; return; }
  res[d.k] = { page: desc(d.back), worker: d.seenByWorker };
};
w.onerror = function (e) { res.__error = String(e.message); };
Object.keys(CASES).forEach(function (k) {
  var v; try { v = (0, eval)(CASES[k]); } catch (e) { res[k] = 'EVALERR:' + e.name; return; }
  try { w.postMessage({ __k: 'echo', k: k, v: v }); } catch (e) { res[k] = 'POSTERR:' + e.name; }
});
res.throwers = {};
Object.keys(THROWERS).forEach(function (k) {
  var v; try { v = (0, eval)(THROWERS[k]); } catch (e) { res.throwers[k] = 'EVALERR:' + e.name; return; }
  try { w.postMessage(v); res.throwers[k] = 'posted'; } catch (e) { res.throwers[k] = e.name + ':' + (e.code === 25); }
});
var ab = new Uint8Array([1,2,3,4]).buffer;
w.postMessage({ __k: 'transfer', v: ab }, [ab]);
res.page_buf_after_transfer = ab.byteLength;
var ab2 = new ArrayBuffer(4);
try { w.postMessage(ab2, [ab2, ab2]); res.dup_transfer = 'posted'; } catch (e) { res.dup_transfer = e.name; }
try { w.postMessage({ x: 1 }, [{}]); res.bad_transfer = 'posted'; } catch (e) { res.bad_transfer = e.name; }
var ab3 = new ArrayBuffer(3); var u3 = new Uint8Array(ab3); w.postMessage({ __k: 'echo', k: 'transfer_view', v: u3 }, [ab3]);
res.view_after_transfer = [ab3.byteLength, u3.length];
setTimeout(function () { window.__out = JSON.stringify(res); }, 800);
`;
const PAGE = `<!doctype html><meta charset=utf-8><body><script>${PAGE_JS}</script>`;
fs.writeFileSync(casesOut, JSON.stringify({ CASES, THROWERS, DESC, WORKER, PAGE_JS }, null, 1));
const srv = http.createServer((q, r) => {
  if (q.url === "/w.js") { r.setHeader("content-type", "text/javascript"); r.end(WORKER); }
  else { r.setHeader("content-type", "text/html"); r.end(PAGE); }
}).listen(0, "127.0.0.1");
await new Promise(r => srv.on("listening", r));
const b = await puppeteer.launch({ executablePath: exe, headless: true, args: ["--no-sandbox", "--disable-gpu"] });
const p = await b.newPage();
await p.goto(`http://127.0.0.1:${srv.address().port}/`);
await p.waitForFunction("window.__out", { timeout: 15000 });
fs.writeFileSync(out, JSON.stringify(JSON.parse(await p.evaluate("window.__out")), null, 1));
console.log(await b.version());
await b.close(); srv.close();
