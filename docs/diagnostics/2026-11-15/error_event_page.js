// Which uncaught exceptions reach the window's `error` event (and window.onerror), by where they are thrown.
var out = { heard: [], onerror: [], rejections: [] };
window.addEventListener("error", function (ev) {
  out.heard.push({ message: String(ev.message), hasError: ev.error instanceof Error, type: Object.prototype.toString.call(ev),
                   file: String(ev.filename).replace(/^.*\//, ""), line: typeof ev.lineno, cancelable: ev.cancelable });
  console.log("HEARD " + ev.message);
});
window.onerror = function (message) { out.onerror.push(String(message)); };
window.addEventListener("unhandledrejection", function (ev) { out.rejections.push(String(ev.reason && ev.reason.message)); });
setTimeout(function () { throw new Error("from a timer"); }, 0);
requestAnimationFrame(function () { throw new Error("from an animation frame"); });
var b = document.createElement("button");
document.body.appendChild(b);
b.addEventListener("click", function () { throw new Error("from a listener"); });
b.dispatchEvent(new Event("click"));
out.afterDispatch = out.heard.length;
Promise.resolve().then(function () { throw new Error("from a promise job"); });
var f = document.createElement("iframe");
f.addEventListener("load", function () { throw new Error("from a frame load listener"); });
f.src = "frame_load_child.html";
document.body.appendChild(f);
var x = new XMLHttpRequest();
x.onload = function () { throw new Error("from an xhr handler"); };
x.open("GET", "frame_load_child.js"); x.send();
setTimeout(function () {
  window.__out = JSON.stringify(out);
  console.log("PROBE " + window.__out);
}, 1200);
throw new Error("from a script's top level");
