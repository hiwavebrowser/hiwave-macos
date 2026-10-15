log.push('b');
export function next() {
  return import('/held/chunk-c.js').then(function (m) { return m.last; });
}
