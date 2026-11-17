// Globals the parent reads through contentWindow: one of each declaration kind.
implicitGlobal = { kind: "assignment" };
var varGlobal = { kind: "var" };
function fnGlobal() { return "function"; }
let letGlobal = { kind: "let" };
class ClassGlobal {}
