// One loop turn plus one own-property write (inline-cache hit).
class Benchmark { runIteration() { var o = { x: 1, y: 2 }; for (var i = 0; i < 2000000; i++) { o.x = i; } return o; } }
