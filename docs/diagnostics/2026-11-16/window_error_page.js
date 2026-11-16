// The window's `error` event for uncaught exceptions, by where they are thrown, and its rules:
// window.onerror's five arguments and its return-true-cancels, preventDefault, a handler that itself
// throws, reportError(), thrown non-errors, a frame's own window, another origin's script.
var out = { events: [], onerror: [], rejections: [], frame: { parent: [], child: [] }, inHandler: { calls: 0 }, order: [] };
function shape(ev) {
  return { message: String(ev.message), type: Object.prototype.toString.call(ev), isTrusted: ev.isTrusted, cancelable: ev.cancelable,
           bubbles: ev.bubbles, target: ev.target === window, file: String(ev.filename).replace(/^.*\//, "").replace(/\?.*$/, ""),
           lineno: ev.lineno > 0, colno: ev.colno > 0, error: ev.error === null ? "null" : ev.error instanceof Error ? ev.error.name : typeof ev.error,
           prevented: ev.defaultPrevented };
}
var tag = "";
window.addEventListener("error", function (ev) {
  if (ev.target !== window) return; // a resource's error event (capture is off; kept for clarity)
  var s = shape(ev); s.where = tag; out.events.push(s);
  if (String(ev.message).indexOf("from a frame") >= 0) out.frame.parent.push(String(ev.message));
});
window.onerror = function (message, source, lineno, colno, error) {
  out.onerror.push({ n: arguments.length, message: String(message), source: String(source).replace(/^.*\//, "").replace(/\?.*$/, ""),
                     lineno: typeof lineno, colno: typeof colno, error: error === null ? "null" : error instanceof Error ? error.name : typeof error,
                     thisIsWindow: this === window });
  out.order.push("onerror");
  if (String(message).indexOf("cancel me") >= 0) return true;
};
window.addEventListener("error", function (ev) {
  out.order.push("listener");
  if (String(ev.message).indexOf("cancel me") >= 0) out.cancelSeen = { prevented: ev.defaultPrevented, returnValue: ev.returnValue };
  if (String(ev.message).indexOf("prevent me") >= 0) { ev.preventDefault(); out.preventSeen = ev.defaultPrevented; }
  if (String(ev.message).indexOf("throw in the handler") >= 0 || String(ev.message).indexOf("from the handler") >= 0) {
    out.inHandler.calls++;
    out.inHandler.messages = (out.inHandler.messages || []).concat(String(ev.message));
    if (out.inHandler.calls < 5) throw new Error("from the handler");
  }
});
window.addEventListener("unhandledrejection", function (ev) { out.rejections.push(String(ev.reason && ev.reason.message)); });

var steps = [
  ["listener", function () { var b = document.createElement("button"); document.body.appendChild(b);
    b.addEventListener("click", function () { throw new Error("from a listener"); }); b.dispatchEvent(new Event("click")); out.syncAfterDispatch = out.events.length; }],
  ["timer", function () { setTimeout(function () { throw new Error("from a timer"); }, 0); }],
  ["frame callback", function () { requestAnimationFrame(function () { throw new Error("from an animation frame"); }); }],
  ["promise job", function () { Promise.resolve().then(function () { throw new Error("from a promise job"); }); }],
  ["onerror returns true", function () { setTimeout(function () { throw new Error("cancel me"); }, 0); }],
  ["preventDefault", function () { setTimeout(function () { throw new Error("prevent me"); }, 0); }],
  ["handler throws", function () { setTimeout(function () { throw new Error("throw in the handler"); }, 0); }],
  ["reportError", function () { out.reportError = typeof self.reportError; if (typeof self.reportError === "function") { var e = new RangeError("reported");
    var before = out.events.length; self.reportError(e); out.reportErrorSync = out.events.length - before; out.reportErrorSame = out.lastError === e; } }],
  ["reportError no argument", function () { try { self.reportError(); out.reportErrorNoArg = "returned"; } catch (e) { out.reportErrorNoArg = e.name; } }],
  ["thrown string", function () { setTimeout(function () { throw "a string"; }, 0); }],
  ["thrown object", function () { setTimeout(function () { throw { message: "an object's message" }; }, 0); }],
  ["thrown null", function () { setTimeout(function () { throw null; }, 0); }],
  ["inline handler attribute", function () { var d = document.createElement("div"); d.setAttribute("onclick", "throw new Error('from an attribute handler')");
    document.body.appendChild(d); d.dispatchEvent(new Event("click")); }],
  ["syntax error", function () { var s = document.createElement("script"); s.textContent = "var = ;"; document.body.appendChild(s); }],
  ["inserted script", function () { var s = document.createElement("script"); s.textContent = "throw new TypeError('from an inserted script')"; document.body.appendChild(s); }],
  ["module", function () { var s = document.createElement("script"); s.type = "module"; s.src = "window_error_module.js"; document.body.appendChild(s); }],
  ["frame", function () { var f = document.createElement("iframe"); f.src = "window_error_child.html"; document.body.appendChild(f); }],
  ["other origin top level", function () { var s = document.createElement("script"); s.src = window.OTHER + "/window_error_other.js"; document.body.appendChild(s); }],
  ["other origin function", function () { setTimeout(function () { if (typeof otherOriginThrows === "function") otherOriginThrows(); else out.otherMissing = true; }, 0); }],
  ["dispatched by the page", function () { var before = out.onerror.length; window.dispatchEvent(new ErrorEvent("error", { message: "made by the page", filename: "f.js", lineno: 3, colno: 4, error: null, cancelable: true }));
    out.pageDispatchCallsOnerror = out.onerror.length - before; }]
];
window.addEventListener("error", function (ev) { out.lastError = ev.error; });
var i = 0;
function next() {
  if (i >= steps.length) { tag = "done"; return setTimeout(function () { window.__out = JSON.stringify(out); console.log("PROBE " + window.__out); }, 300); }
  var step = steps[i++]; tag = step[0];
  try { step[1](); } catch (e) { out.events.push({ where: tag, stepThrew: String(e) }); }
  setTimeout(next, 250);
}
setTimeout(next, 50);
tag = "top level";
throw new Error("from a script's top level");
