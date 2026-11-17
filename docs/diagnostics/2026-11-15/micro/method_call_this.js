// One loop turn plus one method call that reads this.x.
class P { constructor() { this.x = 1; } get1() { return this.x; } }
class Benchmark { runIteration() { var p = new P(), s = 0; for (var i = 0; i < 2000000; i++) { s = p.get1(); } return s; } }
