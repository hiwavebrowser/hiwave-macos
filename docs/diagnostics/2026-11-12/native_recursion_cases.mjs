// Native recursion depth: deep structures handed to built-ins. A page must never kill the
// browser; Chrome throws a catchable RangeError (or, where V8 is iterative, just answers).
// Usage: node native_recursion_cases.mjs <chrome.exe> <cases.json> <results.json>
import puppeteer from "puppeteer-core";
import fs from "fs";
const [exe, casesOut, resultsOut] = process.argv.slice(2);
const N = 200000;
const nest = `var a = []; for (var i = 0; i < ${N}; i++) a = [a];`;
const nestObj = `var o = {}; for (var i = 0; i < ${N}; i++) o = { o: o };`;
const cases = {
  build_only: `${nest} return typeof a;`,
  build_then_garbage: `${nest} var junk = []; for (var j = 0; j < 200000; j++) junk.push({ k: j, v: [j] }); junk = null; return typeof a;`,
  build_object_then_drop: `${nestObj} o = null; var junk = []; for (var j = 0; j < 100000; j++) junk.push([j]); return junk.length;`,
  json_stringify_array: `${nest} return JSON.stringify(a).length;`,
  json_stringify_object: `${nestObj} return JSON.stringify(o).length;`,
  json_stringify_indent: `${nest} return JSON.stringify(a, null, 1).length;`,
  json_stringify_replacer: `${nest} return JSON.stringify(a, function (k, v) { return v; }).length;`,
  flat_infinity: `${nest} return a.flat(Infinity).length;`,
  flat_deep_number: `${nest} return a.flat(${N * 2}).length;`,
  array_to_string: `${nest} return a.toString().length;`,
  array_join: `${nest} return a.join().length;`,
  string_of_array: `${nest} return String(a).length;`,
  concat_to_primitive: `${nest} return (a + '').length;`,
  loose_equals: `${nest} return a == 1;`,
  structured_clone_array: `${nest} return typeof structuredClone(a);`,
  structured_clone_object: `${nestObj} return typeof structuredClone(o);`,
  json_parse_array: `return typeof JSON.parse('['.repeat(${N}) + ']'.repeat(${N}));`,
  json_parse_object: `return typeof JSON.parse('{"a":'.repeat(${N}) + '1' + '}'.repeat(${N}));`,
  json_parse_reviver: `return typeof JSON.parse('['.repeat(${N}) + ']'.repeat(${N}), function (k, v) { return v; });`,
  parse_nested_array_source: `return typeof eval('['.repeat(${N}) + ']'.repeat(${N}));`,
  parse_nested_parens: `return typeof eval('('.repeat(${N}) + '1' + ')'.repeat(${N}));`,
  parse_nested_blocks: `return typeof eval('{'.repeat(${N}) + '}'.repeat(${N}));`,
  parse_nested_functions: `return typeof eval('(function(){'.repeat(${N}) + '})'.repeat(${N}));`,
  parse_long_binary_chain: `return typeof eval('1' + '+1'.repeat(${N}));`,
  parse_nested_template: `return typeof eval('\`' + '\${\`'.repeat(${N}) + '\`}'.repeat(${N}) + '\`');`,
  new_function_nested: `return typeof new Function('return ' + '['.repeat(${N}) + ']'.repeat(${N}));`,
  regex_alt_star: `return /^(?:a|b)*$/.test('ab'.repeat(${N}));`,
  regex_capture_star: `return /^(a|b)*$/.test('ab'.repeat(${N}));`,
  regex_nested_group_source: `return typeof new RegExp('('.repeat(${N}) + 'a' + ')'.repeat(${N}));`,
  regex_lazy_long: `return /^(?:a+?)+$/.test('a'.repeat(${N}));`,
  array_flatmap_nested: `${nest} return a.flatMap(function (x) { return x; }).length;`,
  to_locale_string: `${nest} return a.toLocaleString().length;`,
  object_to_primitive_chain: `var o = {}; for (var i = 0; i < ${N}; i++) { var p = o; o = { toString: function () { return String(p); } }; } return String(o).length;`,
  proxy_chain_get: `var t = {}; for (var i = 0; i < ${N}; i++) t = new Proxy(t, {}); return typeof t.x;`,
  proxy_chain_has: `var t = {}; for (var i = 0; i < ${N}; i++) t = new Proxy(t, {}); return 'x' in t;`,
  bound_function_chain: `var f = function () { return 1; }; for (var i = 0; i < 2000; i++) f = f.bind(null); return f();`,
  prototype_chain_get: `var o = {}; for (var i = 0; i < ${N}; i++) o = Object.create(o); return typeof o.x;`,
  json_stringify_tojson_chain: `var o = { toJSON: function () { return 1; } }; for (var i = 0; i < ${N}; i++) { var q = o; o = { toJSON: function () { return q; } }; } return JSON.stringify(o);`,
  object_assign_getter_chain: `var o = { v: 1 }; for (var i = 0; i < ${N}; i++) { var q = o; o = { get v() { return q.v; } }; } return o.v;`,
  array_sort_comparator_deep: `${nest} return [a, a].sort().length;`,
  string_repeat_replace_callback: `return 'a'.replace(/a/, function f() { return 'b'; });`,
};
fs.writeFileSync(casesOut, JSON.stringify(cases, null, 1));
const b = await puppeteer.launch({ executablePath: exe, headless: true, args: ["--no-sandbox", "--disable-gpu"] });
const res = {};
for (const [k, src] of Object.entries(cases)) {
  const p = await b.newPage();
  res[k] = await Promise.race([
    p.evaluate((src) => { try { return "ok:" + String((new Function(src))()); } catch (e) { return "ERR:" + e.name; } }, src),
    new Promise(r => setTimeout(() => r("TIMEOUT"), 30000)),
  ]).catch(e => "CRASH:" + String(e).slice(0, 60));
  await p.close().catch(() => {});
}
fs.writeFileSync(resultsOut, JSON.stringify(res, null, 1));
console.log(await b.version());
await b.close();
