// One loop turn plus one method read through two prototype levels (a class hierarchy).
class A { m() {} } class B extends A {} class C extends B {}
class Benchmark { runIteration() { var o = new C(), f; for (var i = 0; i < 2000000; i++) { f = o.m; } return f; } }
