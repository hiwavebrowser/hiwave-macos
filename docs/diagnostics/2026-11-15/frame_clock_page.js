// Clocks of a page and its same-origin frame. Relations as booleans; the raw numbers under `raw`.
var near = function (a, b) { return Math.abs(a - b) < 20; };
document.getElementById('f').addEventListener('load', function (e) { window.frameLoadSeen = [e.timeStamp, performance.now()]; });
window.addEventListener('load', function () {
  var cw = document.getElementById('f').contentWindow, r = { raw: {} };
  var po = performance.timeOrigin, co = cw.performance.timeOrigin, D = co - po;
  var a = performance.now(), c = cw.performance.now(), b = performance.now();
  r.raw.originDelta = D; r.raw.parentNow = a; r.raw.childNow = c; r.raw.childFirstRead = cw.firstRead;
  r.now = {
    childOriginIsLater: D >= 100,
    childReadsLessThanParent: c < a,
    parentMinusChildIsTheOriginDelta: Math.abs((a + b) / 2 - c - D) < 2,
    originPlusNowIsOneInstant: Math.abs((po + (a + b) / 2) - (co + c)) < 2,
    childFirstReadIsFromItsOwnOrigin: cw.firstRead >= 0 && cw.firstRead < a - 100
  };
  r.timeline = (function () {
    var p = document.timeline.currentTime, q = cw.document.timeline.currentTime;
    r.raw.timeline = [p, q];
    return { parentMinusChildIsTheOriginDelta: Math.abs(p - q - D) < 20, equal: p === q };
  })();
  // events
  r.ev = {};
  var e1 = new Event('x'), pn = performance.now(), cn = cw.performance.now(), e2 = new cw.Event('x');
  r.raw.ev = { parentMade: e1.timeStamp, childCtor: e2.timeStamp, pn: pn, cn: cn };
  r.ev.madeByParent = { nearParentNow: near(e1.timeStamp, pn), nearChildNow: near(e1.timeStamp, cn) };
  r.ev.madeWithTheFramesConstructor = { nearParentNow: near(e2.timeStamp, pn), nearChildNow: near(e2.timeStamp, cn) };
  var cb = cw.document.getElementById('cb'), seen;
  cb.addEventListener('x', function (e) { seen = e.timeStamp; });
  cb.dispatchEvent(e1);
  r.ev.parentsEventDispatchedInTheFrame = { sameAsBefore: seen === e1.timeStamp, nearParentNow: near(seen, pn), nearChildNow: near(seen, cn) };
  var pseen;
  document.getElementById('pb').addEventListener('x', function (e) { pseen = e.timeStamp; });
  document.getElementById('pb').dispatchEvent(e2);
  r.ev.framesEventDispatchedInTheParent = { sameAsBefore: pseen === e2.timeStamp, nearParentNow: near(pseen, pn), nearChildNow: near(pseen, cn) };
  var get = Object.getOwnPropertyDescriptor(cw.Event.prototype, 'timeStamp').get;
  var viaChild = get.call(e1);
  r.raw.ev.parentMadeViaChildGetter = viaChild;
  r.ev.parentsEventReadWithTheFramesGetter = { sameAsOwnGetter: viaChild === e1.timeStamp, nearParentNow: near(viaChild, pn), nearChildNow: near(viaChild, cn) };
  var click;
  cb.addEventListener('click', function (e) { click = [e.timeStamp, performance.now(), cw.performance.now(), e.isTrusted]; });
  cb.click();
  r.ev.clickMethodInTheFrame = { nearParentNow: near(click[0], click[1]), nearChildNow: near(click[0], click[2]) };
  r.ev.frameWindowLoad = cw.loadSeen ? { nearChildNowThen: near(cw.loadSeen[0], cw.loadSeen[1]) } : null;
  r.ev.iframeElementLoad = window.frameLoadSeen ? { nearParentNowThen: near(window.frameLoadSeen[0], window.frameLoadSeen[1]) } : null;
  window.__trustedClick = function () { return click; };
  // animation frames: the page and the frame ask in the same task, twice
  r.raf = [];
  var round = 0;
  function ask() {
    var got = { order: [] }, n = 0;
    function fin() {
      if (++n < 2) return;
      r.raw['raf' + round] = got;
      r.raf.push({
        equalNumbers: got.p === got.c,
        parentMinusChildIsTheOriginDelta: Math.abs(got.p - got.c - D) < 2,
        notAfterEachDocumentsNow: got.pNow >= got.p && got.cNow >= got.c,
        order: got.order.join()
      });
      if (++round < 2) ask(); else { window.__out = JSON.stringify(r); }
    }
    requestAnimationFrame(function (t) { got.p = t; got.pNow = performance.now(); got.order.push('parent'); fin(); });
    cw.requestAnimationFrame(function (t) { got.c = t; got.cNow = cw.performance.now(); got.order.push('frame'); fin(); });
  }
  ask();
});
