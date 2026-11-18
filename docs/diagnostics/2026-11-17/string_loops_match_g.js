// JS-ROBUST (d), #1252: a global regexp match over a 2^17-character Latin-1 string, one call per iteration.
//   cargo run --release -p rustkit-js --example jsbench -- --iterations 10 docs/diagnostics/2026-11-17/string_loops_match_g.js
//   node [tier flags] docs/diagnostics/2026-11-15/jsbench.mjs --iterations 10 docs/diagnostics/2026-11-17/string_loops_match_g.js
// The string is built at load, outside the timer.
var s = 'xy'.repeat(2 ** 16);
class Benchmark {
  runIteration() {
    var n = s.match(/x/g).length;
    if (n !== 2 ** 16) throw new Error('match_g: ' + n);
  }
}
