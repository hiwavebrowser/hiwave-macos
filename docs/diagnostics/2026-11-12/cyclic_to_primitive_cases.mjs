// Cyclic ToPrimitive/ToString: what Chrome does with a user function that converts its own
// object (RangeError) versus arrays, which V8 guards in join (a cycle contributes "").
// Usage: node cyclic_to_primitive_cases.mjs <chrome.exe> <cases.json> <results.json>
import puppeteer from "puppeteer-core";
import fs from "fs";
const [exe, casesOut, resultsOut] = process.argv.slice(2);
const cases = {
  array_self_string: "var a = [1]; a[1] = a; return String(a);",
  array_self_join: "var a = []; a[0] = a; return JSON.stringify(a.join());",
  array_nested_cycle: "var a = [1, [2]]; a[1][1] = a; return String(a);",
  array_self_concat: "var a = [1]; a[1] = a; return a + '';",
  array_self_tolocale: "var a = [1]; a[1] = a; return a.toLocaleString();",
  array_self_join_sep: "var a = ['x']; a[1] = a; return a.join('-');",
  array_like_join: "var o = { length: 2, 0: 'p' }; o[1] = o; return Array.prototype.join.call(o);",
  array_mutual: "var a = [1], b = [2]; a[1] = b; b[1] = a; return String(a) + '|' + String(b);",
  array_not_cyclic_repeat: "var x = [1]; var a = [x, x]; return String(a);",
  array_ok: "return String([1, [2, 3], [], [[4]]]);",
  array_cycle_then_reuse: "var a = [1]; a[1] = a; String(a); return String(a) + '|' + [a, a].join(';');",
  user_to_string_self: "var o = { toString() { return String(o); } }; try { return 'value:' + String(o); } catch (e) { return 'ERR:' + e.name; }",
  user_to_string_template: "var o = { toString() { return `${o}`; } }; try { return 'value:' + String(o); } catch (e) { return 'ERR:' + e.name; }",
  user_to_string_concat: "var o = { toString() { return o + ''; } }; try { return 'value:' + String(o); } catch (e) { return 'ERR:' + e.name; }",
  user_mutual: "var a = { toString() { return String(b); } }, b = { toString() { return String(a); } }; try { return 'value:' + String(a); } catch (e) { return 'ERR:' + e.name; }",
  user_value_of_self: "var o = { valueOf() { return +o; } }; try { return 'value:' + (+o); } catch (e) { return 'ERR:' + e.name; }",
  user_to_primitive_self: "var o = { [Symbol.toPrimitive]() { return String(o); } }; try { return 'value:' + String(o); } catch (e) { return 'ERR:' + e.name; }",
  user_to_string_counts_calls: "var d = 0; var o = { toString() { d++; if (d > 5) return 'deep'; return String(o); } }; return String(o) + ':' + d;",
  user_inner_once: "var inner = 'unset', o = { toString() { inner = String(o); return 'x'; } }; try { String(o); } catch (e) { return 'ERR:' + e.name; } return 'inner=' + JSON.stringify(inner);",
};
fs.writeFileSync(casesOut, JSON.stringify(cases, null, 1));
const b = await puppeteer.launch({ executablePath: exe, headless: true, args: ["--no-sandbox", "--disable-gpu"] });
const p = await b.newPage();
const res = await p.evaluate((cases) => {
  const out = {};
  for (const [k, src] of Object.entries(cases)) {
    try { out[k] = String((new Function(src))()); } catch (e) { out[k] = "THROWN:" + e.name; }
  }
  return out;
}, cases);
fs.writeFileSync(resultsOut, JSON.stringify(res, null, 1));
console.log(await b.version());
await b.close();
