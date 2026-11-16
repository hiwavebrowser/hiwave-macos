// One loop turn plus one dense array element read.
class Benchmark { runIteration() { var a = [1, 2, 3, 4, 5, 6, 7, 8], s = 0; for (var i = 0; i < 2000000; i++) { s = a[i & 7]; } return s; } }
