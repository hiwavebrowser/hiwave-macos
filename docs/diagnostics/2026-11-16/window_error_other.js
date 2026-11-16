// Served from another origin (another port): what it throws is reported to the page without detail.
function otherOriginThrows() { throw new Error("from another origin's script"); }
function otherOriginEvals() { eval("throw new Error('from another origin, in eval')"); }
function otherOriginThrowsAString() { throw "another origin's string"; }
function otherOriginThrowsAnObject() { throw { theirs: 1 }; }
function otherOriginCalls(f) { f(); }
var otherOriginMade = new Function("throw new Error('in a function another origin made')");
var otherOriginEvalMade = eval("(function () { throw new Error('in a function another origin made with eval'); })");
function otherOriginRejects() { Promise.reject(new Error("another origin's later rejection")); }
Promise.reject(new Error("another origin's rejection"));
throw new Error("from another origin's top level");
