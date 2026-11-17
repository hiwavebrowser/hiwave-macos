// One loop turn plus one two-property object literal (allocation).
class Benchmark { runIteration() { var o; for (var i = 0; i < 2000000; i++) { o = { a: i, b: i }; } return o; } }
