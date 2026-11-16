// Does a document's clock keep real time across a long task? A frame runs a 1500 ms task (started from
// the parent's load handler, as a benchmark runner builds its scene), then ten animation frames. For
// each: performance.now(), Date.now() and the frame timestamp, all since the task began. The clock is
// honest when the three agree; `ahead` is the largest performance.now() minus Date.now().
var f = document.createElement("iframe");
document.body.appendChild(f);
f.onload = function () {
  f.contentWindow.go(1500, 10, function (r) {
    var ahead = r.afterTask[0] - r.afterTask[1], tsAhead = -1e9;
    for (var i = 0; i < r.frames.length; i++) {
      ahead = Math.max(ahead, r.frames[i][0] - r.frames[i][1]);
      tsAhead = Math.max(tsAhead, r.frames[i][2] - r.frames[i][1]);
    }
    var out = { perfAheadOfDateMs: ahead, frameStampAheadOfDateMs: tsAhead, withinAFrame: ahead <= 20 && tsAhead <= 20, raw: r };
    window.__out = JSON.stringify(out);
    console.log("PROBE " + window.__out);
  });
};
f.src = "long_task_clock_child.html";
