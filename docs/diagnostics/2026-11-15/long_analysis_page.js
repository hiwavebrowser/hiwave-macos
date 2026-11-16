// A long, legitimate computation in one task: resample-sort-fit, the shape of a benchmark's end-of-test
// statistics (N rows resampled ITER times, each resample sorted with a comparator and scanned once).
// Reports how long each part took and whether the task ran to its end.
function analysis(rows, iterations) {
  var seed = 12345, n = rows.length, sample = new Array(n), compares = 0, acc = 0;
  function rnd() { seed = (seed * 1103515245 + 12345) & 0x7fffffff; return seed / 0x80000000; }
  var t0 = performance.now(), sortMs = 0, fitMs = 0;
  for (var i = 0; i < iterations; i++) {
    for (var j = 0; j < n; j++) sample[j] = rows[Math.floor(rnd() * n)];
    var a = performance.now();
    sample.sort(function (x, y) { compares++; return x[0] - y[0]; });
    var b = performance.now();
    var sx = 0, sy = 0, sxy = 0, sxx = 0;
    for (j = 0; j < n; j++) { var r = sample[j]; sx += r[0]; sy += r[1]; sxy += r[0] * r[1]; sxx += r[0] * r[0]; }
    acc += (n * sxy - sx * sy) / (n * sxx - sx * sx);
    sortMs += b - a; fitMs += performance.now() - b;
  }
  return { rows: n, iterations: iterations, totalMs: Math.round(performance.now() - t0), sortMs: Math.round(sortMs),
           fitMs: Math.round(fitMs), compares: compares, slope: Math.round(acc / iterations * 1e6) / 1e6 };
}
var out = { runs: [], finished: false };
function rowsOf(n) { var rows = []; for (var i = 0; i < n; i++) rows.push([(i * 7919) % 613, 16 + (i % 13)]); return rows; }
setTimeout(function () {
  out.runs.push(analysis(rowsOf(560), 2500));    // about 10 s of frames
  out.runs.push(analysis(rowsOf(1700), 2500));   // about 30 s of frames
  out.finished = true;
}, 1000);
var polls = 0;
var id = setInterval(function () {
  if (!out.finished && ++polls < 600) return;
  clearInterval(id);
  window.__out = JSON.stringify(out);
  console.log("PROBE " + window.__out);
}, 100);
