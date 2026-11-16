// A page's same-origin frames with and without a `sandbox` attribute: what the page and the frame see.
// Reads only; the answers go to window.__out as JSON. Used by frame_sandbox_chrome.mjs.
var VALUES = { none: null, empty: "", scripts: "allow-scripts", origin: "allow-same-origin", both: "allow-scripts allow-same-origin" };
var INLINE = "<p id=c>child text</p><script>(function(){var name='NAME',r={ran:true};" +
  "try{r.frameElement=window.frameElement===null?'null':typeof window.frameElement}catch(e){r.frameElement='throws '+(e instanceof DOMException?'DOMException':e.name)}" +
  "try{parent.marks[name]=(parent.marks[name]||0)+1;r.parentWrite='ok'}catch(e){r.parentWrite='throws '+(e instanceof DOMException?'DOMException':e.name)}" +
  "try{r.origin=String(window.origin)}catch(e){r.origin='throws '+(e instanceof DOMException?'DOMException':e.name)}" +
  "parent.postMessage(JSON.stringify({name:name,r:r}),'*')})()<\/script>";
function make(id, sandbox, how) {
  var f = document.createElement("iframe");
  f.id = id;
  if (sandbox !== null) f.setAttribute("sandbox", sandbox);
  if (how === "src") f.src = "frame_sandbox_child.html?c=" + id;
  if (how === "srcdoc") f.srcdoc = INLINE.replace("NAME", id);
  count(f);
  document.body.appendChild(f);
  return f;
}
function read(f) {
  var o = { loads: loads[f.id], marked: marks[f.id] || 0, said: said[f.id] || null };
  try { var d = f.contentDocument; o.contentDocument = d === null ? "null" : typeof d; } catch (e) { o.contentDocument = "throws " + kind(e); }
  try { var w = f.contentWindow; o.contentWindow = w === null ? "null" : typeof w; } catch (e) { o.contentWindow = "throws " + kind(e); }
  try { var wd = f.contentWindow.document; o.contentWindowDocument = wd === null ? "null" : typeof wd; } catch (e) { o.contentWindowDocument = "throws " + kind(e); }
  return o;
}
// What was thrown, by class.
function kind(e) { return e instanceof DOMException ? "DOMException" : e.name; }
function later(ms) { return new Promise(function (r) { setTimeout(r, ms); }); }
(async function () {
  var out = { src: {}, srcdoc: {}, blank: {}, markup: {}, afterLoad: {} }, k;
  for (k in VALUES) make("src_" + k, VALUES[k], "src");
  for (k in VALUES) make("srcdoc_" + k, VALUES[k], "srcdoc");
  // about:blank, then the page writes a script into it.
  ["none", "empty", "both"].forEach(function (k) {
    var f = make("blank_" + k, VALUES[k], "blank"), id = f.id;
    try {
      var d = f.contentDocument;
      d.open(); d.write("<script>parent.marks['" + id + "'] = 1<\/script>"); d.close();
      out.blank[k] = { write: "ok" };
    } catch (e) { out.blank[k] = { write: "throws " + kind(e) }; }
  });
  // Frames that come into the document as markup, as copies, or from elsewhere.
  // `_none`: no attribute; `_empty`: sandbox=""; `_removed`: sandbox="", taken off in the next statement.
  function markup(id, sandbox) {
    return "<iframe id=" + id + (sandbox ? " sandbox" : "") + ' src="frame_sandbox_child.html?c=' + id + '"></iframe>';
  }
  function three(prefix, put) {
    [["none", false, false], ["empty", true, false], ["removed", true, true]].forEach(function (c) {
      var id = prefix + "_" + c[0], f = put(id, markup(id, c[1]));
      f = f || document.getElementById(id);
      count(f);
      if (c[2]) f.removeAttribute("sandbox");
    });
  }
  function holder() { return document.body.appendChild(document.createElement("div")); }
  three("inner", function (id, m) { holder().innerHTML = m; });
  three("adjacent", function (id, m) { holder().insertAdjacentHTML("beforeend", m); });
  three("outer", function (id, m) { holder().appendChild(document.createElement("p")).outerHTML = m; });
  // Through a fragment (a template's content).
  three("fragment", function (id, m) { var t = document.createElement("template"); t.innerHTML = m; holder().appendChild(t.content); });
  // Made in a subtree that is not in the document, which goes in afterwards.
  three("connected", function (id, m) { var d = document.createElement("div"); d.innerHTML = m; document.body.appendChild(d); });
  // The same, with the attribute taken off before the subtree goes in.
  (function () {
    var d = document.createElement("div"); d.innerHTML = markup("connected_before", true);
    var f = d.firstChild; count(f); f.removeAttribute("sandbox"); document.body.appendChild(d);
  })();
  // A deep copy of a frame's parent, and of a frame, put in the document.
  three("copy", function (id, m) {
    var d = document.createElement("div"); d.innerHTML = m;
    var c = d.cloneNode(true); document.body.appendChild(c); return c.firstChild;
  });
  // A frame element of another document, adopted.
  three("adopted", function (id, m) {
    var other = document.implementation.createHTMLDocument(""); other.body.innerHTML = m;
    var f = document.adoptNode(other.body.firstChild); document.body.appendChild(f); return f;
  });
  out.inserted = {};
  // The attribute added to, or removed from, a frame that has loaded.
  var added = make("added", null, "src"), removed = make("removed", "", "src");
  await later(1200);
  for (k in VALUES) { out.src[k] = read(document.getElementById("src_" + k)); out.srcdoc[k] = read(document.getElementById("srcdoc_" + k)); }
  ["none", "empty", "both"].forEach(function (k) { var o = read(document.getElementById("blank_" + k)); o.write = out.blank[k].write; out.blank[k] = o; });
  ["mk_removed", "mk_added", "mk_added_src"].forEach(function (id) {
    var o = read(document.getElementById(id));
    if (id === "mk_added_src") o.saidFirst = said.mk_added_src_first || null;
    out.markup[id] = o;
  });
  ["write", "inner", "adjacent", "outer", "fragment", "connected", "copy", "adopted"].forEach(function (p) {
    ["none", "empty", "removed"].forEach(function (c) { out.inserted[p + "_" + c] = read(document.getElementById(p + "_" + c)); });
  });
  out.inserted.connected_before = read(document.getElementById("connected_before"));
  out.afterLoad.added_loaded = read(added);
  out.afterLoad.removed_loaded = read(removed);
  added.setAttribute("sandbox", "");
  removed.removeAttribute("sandbox");
  await later(300);
  // The document made before the change is the one still there.
  out.afterLoad.added_changed = read(added);
  out.afterLoad.added_changed.sameOriginCallStillWorks = (function () { try { return added.contentWindow.document.title; } catch (e) { return "throws " + kind(e); } })();
  out.afterLoad.removed_changed = read(removed);
  // The next navigation takes the attribute as it is then.
  marks = {}; said = {};
  added.src = "frame_sandbox_child.html?c=added";
  removed.src = "frame_sandbox_child.html?c=removed";
  await later(600);
  out.afterLoad.added_navigated = read(added);
  out.afterLoad.removed_navigated = read(removed);
  window.__out = JSON.stringify(out);
})();
