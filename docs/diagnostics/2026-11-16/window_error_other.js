// Served from another origin (another port): what it throws is muted for the page.
function otherOriginThrows() { throw new Error("from another origin's script"); }
throw new Error("from another origin's top level");
