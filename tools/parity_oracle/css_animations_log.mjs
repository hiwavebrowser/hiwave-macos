// What the oracle's Chrome reports for the cases of the engine's
// css_animations_tests (S13b): a forwards fade-in, no fill, an infinite
// spinner, play-state paused/running, iteration count with alternate, a
// delay with `both`, `animation: none` midway, and two animations at once.
// usage: node tools/parity_oracle/css_animations_log.mjs
import { chromium } from 'playwright';
import { getDeterministicLaunchOptions } from './deterministic.mjs';

const page_ = (css) => '<!doctype html><style>body{margin:0;background:#fff}' +
  '#h{width:200px;height:100px;background:#123;color:#fff;font:40px sans-serif}' + css + '</style>' +
  '<div id=h>Placeholder</div>' +
  "<script>var h=document.getElementById('h');window.log=[];" +
  "['animationstart','animationiteration','animationend','animationcancel'].forEach(function(t){" +
  "h.addEventListener(t,function(e){log.push(e.type.slice(9)+':'+e.animationName+'@'+e.elapsedTime)})});</script>";
const FADE = '@keyframes fade{from{opacity:0}to{opacity:1}}';

const browser = await chromium.launch(getDeterministicLaunchOptions());
const page = await browser.newPage({ viewport: { width: 800, height: 600 } });
console.log('chrome ' + browser.version());
const wait = (ms) => page.waitForTimeout(ms);
const ev = (f, arg) => page.evaluate(f, arg);
const state = () => ev(() => [getComputedStyle(h).opacity, log.join()].join(' | '));

await page.setContent(page_('#h{opacity:0;animation:fade 200ms linear forwards}' + FADE));
await wait(100);
console.log('forwards mid: ' + await state());
await wait(200);
console.log('forwards end: ' + await state());
await wait(500);
console.log('forwards later: ' + await state());

await page.setContent(page_('#h{opacity:.5;animation:fade 100ms linear}' + FADE));
await wait(50);
console.log('no fill mid: ' + await state());
await wait(200);
console.log('no fill end: ' + await state());

await page.setContent(page_('#h{animation:spin 1s linear infinite}@keyframes spin{to{transform:rotate(360deg)}}'));
await wait(250);
console.log('spin 250: ' + await ev(() => getComputedStyle(h).transform));
await wait(250);
console.log('spin 500: ' + await ev(() => getComputedStyle(h).transform));
await wait(2000);
console.log('spin 2500: ' + await ev(() => log.join()));

await page.setContent(page_('#h{opacity:0;animation:fade 200ms linear forwards}' + FADE));
await wait(100);
await ev(() => { h.style.animationPlayState = 'paused'; });
await wait(16);
const held = await ev(() => getComputedStyle(h).opacity);
await wait(300);
console.log('paused: held ' + held + ', now ' + await state());
await ev(() => { h.style.animationPlayState = 'running'; });
await wait(300);
console.log('resumed: ' + await state());

await page.setContent(page_('#h{opacity:0;animation:fade 100ms linear 2 alternate}' + FADE));
await wait(50);
console.log('alternate 50: ' + await state());
await wait(100);
console.log('alternate 150: ' + await state());
await wait(200);
console.log('alternate end: ' + await state());

await page.setContent(page_('#h{opacity:0;animation:up 100ms linear 100ms both}@keyframes up{from{opacity:.5}to{opacity:1}}'));
await wait(50);
console.log('both 50: ' + await state());
await wait(100);
console.log('both 150: ' + await state());
await wait(200);
console.log('both end: ' + await state());

await page.setContent(page_('#h{opacity:0;animation:fade 200ms linear forwards}' + FADE));
await wait(100);
await ev(() => { h.style.animation = 'none'; });
await wait(32);
console.log('none: ' + await state());
await wait(300);
console.log('none later: ' + await state());

await page.setContent(page_('#h{opacity:0;animation:fade 100ms linear forwards, slide 100ms linear forwards}' +
  '@keyframes fade{to{opacity:1}}@keyframes slide{to{transform:translateX(100px)}}'));
await wait(200);
console.log('two: ' + await ev(() => [getComputedStyle(h).opacity, getComputedStyle(h).transform, log.sort().join()].join(' | ')));

await browser.close();
