// JS-ROBUST (d), #1252: split over a 2^17-character Latin-1 string, one call per iteration.
//   cargo run --release -p rustkit-js --example jsbench -- --iterations 10 docs/diagnostics/2026-11-17/string_loops_split.js
//   node [tier flags] docs/diagnostics/2026-11-15/jsbench.mjs --iterations 10 docs/diagnostics/2026-11-17/string_loops_split.js
// The string is built at load, outside the timer.
var s = 'xy'.repeat(2 ** 16);
class Benchmark {
  runIteration() {
    var n = s.split('x').length;
    if (n !== 2 ** 16 + 1) throw new Error('split: ' + n);
  }
}
