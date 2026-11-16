// One loop turn plus three local arithmetic ops.
class Benchmark { runIteration() { var a = 1, b = 2, c = 0; for (var i = 0; i < 2000000; i++) { c = (a + b) * c - i; } return c; } }
