// Does a clipper keep its clip over an absolute box under a positioned wrapper, in the pinned Chrome
// (CfT 148)? The second review of #1151 (HIGH 5): a box lifted to its stacking context's list must still
// be clipped by an overflow/clip-path/border-radius clipper that holds its containing block, and must
// not be clipped by one that does not. For each case: is the probe point red (the box `r` shows there)
// in a screenshot, and which element elementFromPoint names there.
// The third review (HIGH 7): a lifted stacking context that is not positioned (opacity) between the box
// that holds the absolute one and the absolute box itself; a scrolled clipper; and a transform or filter
// as the containing block of absolute and fixed boxes. A case may carry a fourth item, `[id, scrollTop]`:
// that element is scrolled before measuring.
// Usage: node lifted_clip_cases.mjs <chrome.exe> <cases.json> <results.json>
import puppeteer from "puppeteer-core";
import fs from "fs";
const [exe, casesOut, resultsOut] = process.argv.slice(2);
const R = "id=r style='position:absolute;left:150px;top:0;width:50px;height:50px;background:rgb(255,0,0)'";
const RC = "id=r style='position:absolute;left:0;top:0;width:50px;height:50px;background:rgb(255,0,0)'";
const clip = (extra) => `id=c style='width:100px;height:100px;background:rgb(0,0,255);${extra}'`;
// [body, x, y]: the probe point lies outside the clipper unless noted.
const cases = {
  // Containing block INSIDE the clipper: Chrome clips the box.
  opacity_overflow_hidden_rel_inside: [`<div ${clip("opacity:.99;overflow:hidden")}><div style='position:relative'><div ${R}></div></div></div>`, 175, 25],
  transform_overflow_hidden_rel_inside: [`<div ${clip("transform:translateX(0);overflow:hidden")}><div style='position:relative'><div ${R}></div></div></div>`, 175, 25],
  root_overflow_hidden_rel_inside: [`<div ${clip("overflow:hidden")}><div style='position:relative'><div ${R}></div></div></div>`, 175, 25],
  opacity_overflow_auto_rel_inside: [`<div ${clip("opacity:.99;overflow:auto;scrollbar-width:none")}><div style='position:relative'><div ${R}></div></div></div>`, 175, 25],
  opacity_overflow_scroll_rel_inside: [`<div ${clip("opacity:.99;overflow:scroll;scrollbar-width:none")}><div style='position:relative'><div ${R}></div></div></div>`, 175, 25],
  opacity_clip_path_rel_inside: [`<div ${clip("opacity:.99;clip-path:inset(0)")}><div style='position:relative'><div ${R}></div></div></div>`, 175, 25],
  root_clip_path_rel_inside: [`<div ${clip("clip-path:inset(0)")}><div style='position:relative'><div ${R}></div></div></div>`, 175, 25],
  // The rounded corner: (3, 3) is outside a 50px radius.
  opacity_border_radius_rel_inside: [`<div ${clip("opacity:.99;overflow:hidden;border-radius:50px")}><div style='position:relative'><div ${RC}></div></div></div>`, 3, 3],
  root_border_radius_rel_inside: [`<div ${clip("overflow:hidden;border-radius:50px")}><div style='position:relative'><div ${RC}></div></div></div>`, 3, 3],
  // The clipper is itself the containing block (positioned): clipped.
  opacity_overflow_hidden_positioned_clipper: [`<div style='opacity:.99'><div ${clip("position:relative;overflow:hidden")}><div style='position:relative'><div ${R}></div></div></div></div>`, 175, 25],
  // Containing block OUTSIDE the clipper: overflow does not clip the box (clip-path still does).
  opacity_rel_outside_overflow_hidden: [`<div style='opacity:.99;position:relative'><div ${clip("overflow:hidden")}><div ${R}></div></div></div>`, 175, 25],
  root_rel_outside_overflow_hidden: [`<div style='position:relative'><div ${clip("overflow:hidden")}><div ${R}></div></div></div>`, 175, 25],
  opacity_rel_outside_overflow_hidden_rel_between: [`<div style='opacity:.99;position:relative'><div ${clip("overflow:hidden")}><div style='position:static'><div ${R}></div></div></div></div>`, 175, 25],
  opacity_rel_outside_clip_path: [`<div style='opacity:.99;position:relative'><div ${clip("clip-path:inset(0)")}><div ${R}></div></div></div>`, 175, 25],
  // A positioned box between: its own containing block chain is inside, so clipped.
  opacity_overflow_hidden_abs_in_abs: [`<div ${clip("opacity:.99;overflow:hidden;position:relative")}><div style='position:absolute;left:0;top:0'><div ${R}></div></div></div>`, 175, 25],
  // In the clipper's border, outside its padding box (the overflow clip): the box is clipped there, so
  // the hit is the clipper. The probe (5, 5) is inside the clipper's border box.
  opacity_overflow_hidden_rel_inside_border_area: [`<div ${clip("opacity:.99;overflow:hidden;border:20px solid rgb(0,0,255)")}><div style='position:relative'><div id=r style='position:absolute;left:-20px;top:-20px;width:50px;height:50px;background:rgb(255,0,0)'></div></div></div>`, 5, 5],
  root_overflow_hidden_rel_inside_border_area: [`<div ${clip("overflow:hidden;border:20px solid rgb(0,0,255)")}><div style='position:relative'><div id=r style='position:absolute;left:-20px;top:-20px;width:50px;height:50px;background:rgb(255,0,0)'></div></div></div>`, 5, 5],
  // A fixed box escapes overflow clipping of a non-transformed ancestor.
  opacity_overflow_hidden_fixed_inside: [`<div ${clip("opacity:.99;overflow:hidden")}><div style='position:relative'><div id=r style='position:fixed;left:150px;top:0;width:50px;height:50px;background:rgb(255,0,0)'></div></div></div>`, 175, 25],
  // HIGH 7: an opacity layer between the relative box and the absolute one.
  opacity_overflow_hidden_rel_opacity_between: [`<div ${clip("opacity:.99;overflow:hidden")}><div style='position:relative'><div style='opacity:.99'><div ${R}></div></div></div></div>`, 175, 25],
  root_overflow_hidden_rel_opacity_between: [`<div ${clip("overflow:hidden")}><div style='position:relative'><div style='opacity:.99'><div ${R}></div></div></div></div>`, 175, 25],
  opacity_rel_outside_overflow_hidden_opacity_between: [`<div style='opacity:.99;position:relative'><div ${clip("overflow:hidden")}><div style='opacity:.99'><div ${R}></div></div></div></div>`, 175, 25],
  // A scrolled clipper (scrollTop 30): the absolute box, at top 40 in a relative box, shows at y 10..30.
  opacity_overflow_auto_scrolled_rel_opacity_between: [`<div ${clip("opacity:.99;overflow:auto;scrollbar-width:none")}><div style='position:relative;height:300px'><div style='opacity:.99'><div id=r style='position:absolute;left:0;top:40px;width:50px;height:20px;background:rgb(255,0,0)'></div></div></div></div>`, 25, 15, ['c', 30]],
  opacity_overflow_auto_scrolled_rel_abs: [`<div ${clip("opacity:.99;overflow:auto;scrollbar-width:none")}><div style='position:relative;height:300px'><div id=r style='position:absolute;left:0;top:40px;width:50px;height:20px;background:rgb(255,0,0)'></div></div></div>`, 25, 15, ['c', 30]],
  // A transform or a filter is the containing block of absolute and fixed boxes inside it.
  opacity_overflow_hidden_transform_holds_abs: [`<div ${clip("opacity:.99;overflow:hidden")}><div style='transform:translateX(0)'><div ${R}></div></div></div>`, 175, 25],
  opacity_overflow_hidden_filter_holds_abs: [`<div ${clip("opacity:.99;overflow:hidden")}><div style='filter:grayscale(0)'><div ${R}></div></div></div>`, 175, 25],
  opacity_overflow_hidden_transform_holds_fixed: [`<div ${clip("opacity:.99;overflow:hidden")}><div style='transform:translateX(0)'><div id=r style='position:fixed;left:150px;top:0;width:50px;height:50px;background:rgb(255,0,0)'></div></div></div>`, 175, 25],
  opacity_overflow_hidden_filter_holds_fixed: [`<div ${clip("opacity:.99;overflow:hidden")}><div style='filter:grayscale(0)'><div id=r style='position:fixed;left:150px;top:0;width:50px;height:50px;background:rgb(255,0,0)'></div></div></div>`, 175, 25],
  root_overflow_hidden_rel_transform_holds_fixed: [`<div ${clip("overflow:hidden")}><div style='position:relative'><div style='transform:translateX(0)'><div id=r style='position:fixed;left:150px;top:0;width:50px;height:50px;background:rgb(255,0,0)'></div></div></div></div>`, 175, 25],
};
fs.writeFileSync(casesOut, JSON.stringify(cases, null, 1));
const b = await puppeteer.launch({ executablePath: exe, headless: true, args: ["--no-sandbox", "--disable-gpu"] });
const p = await b.newPage();
await p.setViewport({ width: 400, height: 300 });
const res = {};
for (const [k, [body, x, y, setup]] of Object.entries(cases)) {
  await p.setContent(`<!doctype html><html><head><style>body{margin:0;background:#fff}</style></head><body>${body}</body></html>`);
  if (setup) await p.evaluate(([id, top]) => { document.getElementById(id).scrollTop = top; }, setup);
  const hit = await p.evaluate((x, y) => { const e = document.elementFromPoint(x, y); return e ? e.id || e.tagName : null; }, x, y);
  const png = await p.screenshot({ clip: { x, y, width: 1, height: 1 }, encoding: "base64" });
  const px = await p.evaluate(async (data) => {
    const img = new Image(); img.src = "data:image/png;base64," + data; await img.decode();
    const c = document.createElement("canvas"); c.width = 1; c.height = 1; const g = c.getContext("2d");
    g.drawImage(img, 0, 0); const d = g.getImageData(0, 0, 1, 1).data; return [d[0], d[1], d[2]];
  }, png);
  const red = px[0] > 200 && px[1] < 50 && px[2] < 50;
  res[k] = { red, hit, px };
}
fs.writeFileSync(resultsOut, JSON.stringify(res, null, 1));
console.log(await b.version());
await b.close();
