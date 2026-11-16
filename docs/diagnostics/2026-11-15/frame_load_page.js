// How many `load` events an iframe fires, and what the parent sees through contentWindow at each one.
// Three ways of making the frame; each records one entry per load event.
var out = { cases: {}, errors: [] };
window.addEventListener("error", function (ev) { out.errors.push(String(ev.message)); });
function see(frame, index) {
  var w = frame.contentWindow, d = frame.contentDocument, r = {};
  try {
    r.href = String(w.location.href).replace(/^.*\//, "");
    r.readyState = d.readyState;
    r.implicitGlobal = typeof w.implicitGlobal;
    r.varGlobal = typeof w.varGlobal;
    r.fnGlobal = typeof w.fnGlobal;
    r.letGlobal = typeof w.letGlobal;
    r.ClassGlobal = typeof w.ClassGlobal;
    r.sameWindowAsFramesEntry = w === window.frames[index];
    r.stage = !!d.getElementById("stage");
  } catch (e) { r.threw = String(e && e.name); }
  return r;
}
function make(name, build) {
  var loads = out.cases[name] = [], index = Object.keys(out.cases).length - 1;
  var host = document.createElement("div");
  document.getElementById("host").appendChild(host);
  build(host, function (ev) { var r = see(ev.target, index); loads.push(r); console.log("LOAD " + name + " " + JSON.stringify(r)); });
}
// 1. insert with no src, THEN set onload, THEN set src (a benchmark runner's order)
make("insertThenOnloadThenSrc", function (host, on) {
  var f = document.createElement("iframe");
  host.insertBefore(f, host.firstChild);
  f.onload = on;
  f.src = "frame_load_child.html";
});
// 2. listener first, insert with no src, never navigated
make("listenerThenInsertNoSrc", function (host, on) {
  var f = document.createElement("iframe");
  f.addEventListener("load", on);
  host.appendChild(f);
});
// 3. listener and src first, then insert
make("listenerSrcThenInsert", function (host, on) {
  var f = document.createElement("iframe");
  f.addEventListener("load", on);
  f.src = "frame_load_child.html";
  host.appendChild(f);
});
// 4. listener first, insert with no src, then src in the same task
make("listenerThenInsertThenSrc", function (host, on) {
  var f = document.createElement("iframe");
  f.addEventListener("load", on);
  host.appendChild(f);
  f.src = "frame_load_child.html";
});
out.syncCounts = {};
for (var k in out.cases) out.syncCounts[k] = out.cases[k].length;
setTimeout(function () {
  window.__out = JSON.stringify(out);
  console.log("PROBE " + window.__out);
}, 1500);
