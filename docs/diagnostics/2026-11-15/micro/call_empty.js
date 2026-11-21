// One loop turn plus one call to an empty function with one argument.
function f(a) { return a; }
class Benchmark { runIteration() { var s = 0; for (var i = 0; i < 2000000; i++) { s = f(i); } return s; } }
