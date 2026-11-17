// One loop turn plus one own-property read on a monomorphic object (inline-cache hit).
class Benchmark { runIteration() { var o = { x: 1, y: 2 }, s = 0; for (var i = 0; i < 2000000; i++) { s = o.x; } return s; } }
