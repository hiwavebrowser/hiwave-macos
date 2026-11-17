// JIT-plan measurement harness, V8 side: the same workload files, loaded as scripts in the main
// context (a vm.createContext global goes through interceptors and slows every tier), then
// `new Benchmark().runIteration()` timed per call, exactly as crates/rustkit-js/examples/jsbench.rs.
// The V8 tier is chosen with node flags: --jitless (Ignition interpreter only),
// --no-maglev --no-turbofan (Ignition + Sparkplug baseline), or none (all tiers).
//   node [tier flags] docs/diagnostics/2026-11-15/jsbench.mjs --iterations 10 [--data input.js] a.js b.js ...
import fs from 'node:fs';
import vm from 'node:vm';
const args = process.argv.slice(2);
let iterations = 10;
const files = [];
const data = {};
for (let i = 0; i < args.length; i++) {
  if (args[i] === '--iterations') iterations = Number(args[++i]);
  else if (args[i] === '--data') { const p = args[++i]; data[p] = fs.readFileSync(p, 'utf8'); }
  else files.push(args[i]);
}
globalThis.isInBrowser = false;
globalThis.read = (p) => { if (!(p in data)) throw new Error('read: ' + p + ' not given with --data'); return data[p]; };
const t0 = performance.now();
for (const f of files) vm.runInThisContext(fs.readFileSync(f, 'utf8'), { filename: f });
const loadMs = performance.now() - t0;
vm.runInThisContext('globalThis.__bench = new Benchmark();');
const run = vm.runInThisContext('(function(){ __bench.runIteration(); })');
const times = [];
for (let i = 0; i < iterations; i++) {
  const t = performance.now();
  run();
  times.push(performance.now() - t);
}
console.log(JSON.stringify({ engine: 'v8 ' + process.execArgv.join(' '), load_ms: +loadMs.toFixed(1), iteration_ms: times.map(t => +t.toFixed(2)) }));
