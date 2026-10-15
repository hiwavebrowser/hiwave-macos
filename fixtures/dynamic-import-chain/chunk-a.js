// The first chunk: imports the next on demand, as a router would.
log.push('a:' + document.getElementById('p').textContent);
export function next() {
  return import('/held/chunk-b.js').then(function (m) { return m.next(); });
}
