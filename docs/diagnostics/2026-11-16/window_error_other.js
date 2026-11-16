// Served from another origin (another port): what it throws is muted for the page.
function otherOriginThrows() { throw new Error("from another origin's script"); }
function otherOriginEvals() { eval("throw new Error('from another origin, in eval')"); }
function otherOriginThrowsAString() { throw "another origin's string"; }
Promise.reject(new Error("another origin's rejection"));
throw new Error("from another origin's top level");
