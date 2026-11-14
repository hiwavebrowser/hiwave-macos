// Paint order of overlapping positioned layers with z-index, position: fixed among them, in the pinned
// Chrome (CfT 148). Two boxes overlap at (150, 150): A (red) comes first in the document, B (blue) later.
// For each case: which one is on top there, by the pixel's colour in a screenshot, and by elementFromPoint.
// bing.com's later state paints its fixed background-photo layers over its fixed header (batch 75).
// Usage: node fixed_zindex_cases.mjs <chrome.exe> <cases.json> <results.json>
import puppeteer from "puppeteer-core";
import fs from "fs";
const [exe, casesOut, resultsOut] = process.argv.slice(2);
const A = "id=a style='left:100px;top:100px;width:100px;height:100px;background:rgb(255,0,0);";
const B = "id=b style='left:120px;top:120px;width:100px;height:100px;background:rgb(0,0,255);";
const cases = {
  fixed_z2_over_fixed_z1_later: `<div ${A}position:fixed;z-index:2'></div><div ${B}position:fixed;z-index:1'></div>`,
  fixed_auto_over_fixed_negative_later: `<div ${A}position:fixed'></div><div ${B}position:fixed;z-index:-1'></div>`,
  fixed_auto_then_fixed_auto: `<div ${A}position:fixed'></div><div ${B}position:fixed'></div>`,
  fixed_z1_over_fixed_auto_later: `<div ${A}position:fixed;z-index:1'></div><div ${B}position:fixed'></div>`,
  fixed_z0_then_fixed_auto: `<div ${A}position:fixed;z-index:0'></div><div ${B}position:fixed'></div>`,
  fixed_z5_in_static_wrapper_over_fixed_z1: `<div><div ${A}position:fixed;z-index:5'></div></div><div ${B}position:fixed;z-index:1'></div>`,
  fixed_z9_in_z0_context_under_fixed_z1: `<div style='position:relative;z-index:0'><div ${A}position:fixed;z-index:9'></div></div><div ${B}position:fixed;z-index:1'></div>`,
  absolute_z2_over_fixed_z1_later: `<div ${A}position:absolute;z-index:2'></div><div ${B}position:fixed;z-index:1'></div>`,
  fixed_header_auto_over_fixed_bg_negative_in_wrapper: `<div ${A}position:fixed'></div><div><div ${B}position:fixed;z-index:-1'></div></div>`,
  // R1 on #1151. A static flex item with a z-index paints as if positioned (netflix's hero).
  flex_item_z3_over_earlier_absolute: `<div style='position:relative;display:flex;width:300px;height:300px'><div id=a style='position:absolute;inset:0;background:rgb(255,0,0)'></div><div id=b style='z-index:3;width:300px;height:300px;flex:none;background:rgb(0,0,255)'></div></div>`,
  flex_item_z3_over_absolute_one_layer_down: `<div style='position:relative;display:flex;width:300px;height:300px'><div style='position:absolute;inset:0'><div id=a style='width:300px;height:300px;background:rgb(255,0,0)'></div></div><div id=b style='z-index:3;width:300px;height:300px;flex:none;background:rgb(0,0,255)'></div></div>`,
  // A non-positioned stacking context paints at z 0 in tree order with the positioned boxes.
  opacity_block_after_earlier_absolute_in_wrapper: `<div><div id=a style='position:absolute;left:0;top:0;width:300px;height:300px;background:rgb(255,0,0)'></div></div><div id=b style='opacity:.99;width:300px;height:300px;background:rgb(0,0,255)'></div>`,
  transform_block_after_earlier_absolute_in_wrapper: `<div><div id=a style='position:absolute;left:0;top:0;width:300px;height:300px;background:rgb(255,0,0)'></div></div><div id=b style='transform:translateX(0);width:300px;height:300px;background:rgb(0,0,255)'></div>`,
  // body { overflow-x: hidden } does not clip (the root's overflow is visible: it goes to the viewport).
  body_overflow_x_hidden_fixed_z5_in_wrapper: `<style>body{overflow-x:hidden}</style><div><div ${A}position:fixed;z-index:5'></div></div><div ${B}position:fixed;z-index:1'></div>`,
  // Absolute boxes: the picture and elementFromPoint must agree.
  absolute_z5_in_static_wrapper_over_absolute_z1: `<div><div ${A}position:absolute;z-index:5'></div></div><div ${B}position:absolute;z-index:1'></div>`,
  // Not lifted by this change (a clipping wrapper stops the lift); recorded so the test names it.
  fixed_z5_in_overflow_hidden_wrapper_over_fixed_z1: `<div style='overflow:hidden;height:10px'><div ${A}position:fixed;z-index:5'></div></div><div ${B}position:fixed;z-index:1'></div>`,
};
fs.writeFileSync(casesOut, JSON.stringify(cases, null, 1));
const b = await puppeteer.launch({ executablePath: exe, headless: true, args: ["--no-sandbox", "--disable-gpu"] });
const p = await b.newPage();
await p.setViewport({ width: 400, height: 300 });
const res = {};
for (const [k, body] of Object.entries(cases)) {
  await p.setContent(`<!doctype html><html><head><style>body{margin:0;background:#fff}</style></head><body>${body}</body></html>`);
  const hit = await p.evaluate(() => { const e = document.elementFromPoint(150, 150); return e ? e.id || e.tagName : null; });
  const png = await p.screenshot({ clip: { x: 150, y: 150, width: 1, height: 1 }, encoding: "base64" });
  const px = await p.evaluate(async (data) => {
    const img = new Image(); img.src = "data:image/png;base64," + data; await img.decode();
    const c = document.createElement("canvas"); c.width = 1; c.height = 1; const g = c.getContext("2d");
    g.drawImage(img, 0, 0); const d = g.getImageData(0, 0, 1, 1).data; return [d[0], d[1], d[2]];
  }, png);
  const top = px[0] > 200 && px[2] < 50 ? "a" : px[2] > 200 && px[0] < 50 ? "b" : "other:" + px.join(",");
  res[k] = { top, hit };
}
fs.writeFileSync(resultsOut, JSON.stringify(res, null, 1));
console.log(await b.version());
await b.close();
