// Installed in Chrome before the page's scripts. Records the fields the
// CDP session does not already have: readyState, DOMContentLoaded, load,
// console errors, large MutationObserver records, and an Error.stack sample.
// Script execute/load/error is collected by chrome_trace.mjs via CDP.
// # sourceURL=replay-probe.js
(function () {
  if (window.__pageTrace) return;
  var t0 = (typeof performance !== "undefined" && performance.now) ? performance.now() : Date.now();
  function now() {
    var n = (typeof performance !== "undefined" && performance.now) ? performance.now() : Date.now();
    return Math.max(0, Math.round(n - t0));
  }
  var trace = {
    ready_state: [],
    dcl_ms: null,
    load_ms: null,
    console: [],
    mutations: [],
    max_js_stack: 0
  };
  window.__pageTrace = trace;
  function sample() {
    var depth = 0;
    try {
      var stack = (new Error()).stack || "";
      depth = stack.split("\n").filter(Boolean).length;
    } catch (e) {}
    if (depth > trace.max_js_stack) trace.max_js_stack = depth;
  }
  var last = "";
  function noteReady() {
    var state = "";
    try { state = document.readyState || ""; } catch (e) { return; }
    if (!state || state === last) return;
    last = state;
    var t = now();
    trace.ready_state.push({ state: state, t_ms: t });
    if (state === "interactive" && trace.dcl_ms == null) trace.dcl_ms = t;
    if (state === "complete" && trace.load_ms == null) trace.load_ms = t;
    sample();
  }
  noteReady();
  document.addEventListener("readystatechange", noteReady);
  document.addEventListener("DOMContentLoaded", function () {
    if (trace.dcl_ms == null) trace.dcl_ms = now();
    sample();
  });
  window.addEventListener("load", function () {
    if (trace.load_ms == null) trace.load_ms = now();
    sample();
  });
  function count(node) {
    var n = 1;
    var kids = node && node.childNodes;
    if (!kids) return n;
    for (var i = 0; i < kids.length; i++) n += count(kids[i]);
    return n;
  }
  function nameOf(node) {
    if (!node) return "";
    if (node.nodeType === 1 && node.tagName) return String(node.tagName).toUpperCase();
    return String(node.nodeName || "");
  }
  if (typeof MutationObserver === "function") {
    var mo = new MutationObserver(function (records) {
      sample();
      for (var i = 0; i < records.length; i++) {
        var record = records[i];
        var added = 0;
        var removed = 0;
        var a = record.addedNodes || [];
        var b = record.removedNodes || [];
        for (var j = 0; j < a.length; j++) added += count(a[j]);
        for (var k = 0; k < b.length; k++) removed += count(b[k]);
        var target = nameOf(record.target);
        var t = now();
        if (added > 50) trace.mutations.push({ t_ms: t, type: "add", nodes: added, target: target });
        if (removed > 50) trace.mutations.push({ t_ms: t, type: "remove", nodes: removed, target: target });
      }
    });
    mo.observe(document.documentElement || document, { childList: true, subtree: true });
  }
  var orig = console.error ? console.error.bind(console) : function () {};
  console.error = function () {
    var parts = [];
    for (var i = 0; i < arguments.length; i++) parts.push(String(arguments[i]));
    trace.console.push({ level: "error", text: parts.join(" ").slice(0, 400), t_ms: now() });
    sample();
    return orig.apply(console, arguments);
  };
  window.addEventListener("error", function (event) {
    var text = String((event && (event.message || event.error)) || "error");
    trace.console.push({ level: "error", text: text.slice(0, 400), t_ms: now() });
    sample();
  });
})();
