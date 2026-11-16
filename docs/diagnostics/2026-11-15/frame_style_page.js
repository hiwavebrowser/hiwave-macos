// Is a frame's external stylesheet applied by the time the frame's `load` fires at its element?
// The child sizes #stage to its viewport with a linked sheet; the parent measures it in the load handler.
var out = {};
var f = document.createElement("iframe");
f.style.width = "400px"; f.style.height = "300px"; f.style.border = "0";
f.onload = function () {
  var d = f.contentDocument, w = f.contentWindow, s = d.getElementById("stage"), r = s.getBoundingClientRect();
  out.atLoad = Math.round(r.width) + "x" + Math.round(r.height) + " " + w.getComputedStyle(s).position;
  out.sheets = d.styleSheets.length;
  out.atScript = w.atScript;
  out.viewport = w.innerWidth + "x" + w.innerHeight;
  window.__out = JSON.stringify(out);
  console.log("PROBE " + window.__out);
};
f.src = "frame_style_child.html";
document.body.appendChild(f);
