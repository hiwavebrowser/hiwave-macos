// What the oracle's Chrome reports for the cases of the engine's
// css_transitions_tests (S13a): a class toggle over `transition: opacity
// 100ms linear` (events and the midway value), cancel by `transition:
// none` and by removal, a midway retarget, `all` over width, colour and
// transform, and a delay.
// usage: node tools/parity_oracle/css_transitions_log.mjs
import { chromium } from 'playwright';
import { getDeterministicLaunchOptions } from './deterministic.mjs';

const PAGE = '<!doctype html><style>body{margin:0}' +
  '#h{width:100px;height:100px;background:rgb(0,0,255);opacity:0;transition:opacity 100ms linear}' +
  '#h.on{opacity:1}</style><div id=h></div>' +
  "<script>var h=document.getElementById('h');window.log=[];" +
  "['transitionrun','transitionstart','transitionend','transitioncancel'].forEach(function(t){" +
  "h.addEventListener(t,function(e){log.push(e.type.slice(10)+':'+e.propertyName)})});</script>";

const browser = await chromium.launch(getDeterministicLaunchOptions());
const page = await browser.newPage({ viewport: { width: 800, height: 600 } });
console.log('chrome ' + browser.version());
const wait = (ms) => page.waitForTimeout(ms);
const ev = (f, arg) => page.evaluate(f, arg);
const state = () => ev(() => [getComputedStyle(h).opacity, log.join()].join(' | '));

await page.setContent(PAGE);
await wait(50);
await ev(() => h.classList.add('on'));
await wait(48);
console.log('toggle mid: ' + await state());
await wait(160);
console.log('toggle end: ' + await state());
await wait(200);
console.log('toggle later: ' + await state());

await page.setContent(PAGE);
await wait(50);
await ev(() => h.classList.add('on'));
await wait(50);
console.log('midway read: ' + await state());

await page.setContent(PAGE);
await wait(50);
await ev(() => h.classList.add('on'));
await wait(48);
await ev(() => { h.style.transition = 'none'; });
await wait(200);
console.log('transition none: ' + await state());

await page.setContent(PAGE);
await wait(50);
await ev(() => h.classList.add('on'));
await wait(48);
await ev(() => h.remove());
await wait(200);
console.log('removed: ' + await ev(() => log.join()));

await page.setContent(PAGE);
await wait(50);
await ev(() => h.classList.add('on'));
await wait(48);
const reached = await ev(() => getComputedStyle(h).opacity);
await ev(() => h.classList.remove('on'));
await wait(16);
console.log('retarget: reached ' + reached + ', back ' + await state());
await wait(200);
console.log('retarget end: ' + await state());

const ALL = '<!doctype html><style>body{margin:0}' +
  '#h{width:100px;height:20px;background:rgb(0,0,255);transform:translateX(0px);transition:all 100ms linear}' +
  '#h.on{width:200px;background:rgb(255,0,0);transform:translateX(100px)}</style><div id=h></div>' +
  "<script>var h=document.getElementById('h');window.log=[];" +
  "['transitionend'].forEach(function(t){h.addEventListener(t,function(e){log.push(e.type.slice(10)+':'+e.propertyName)})});</script>";
await page.setContent(ALL);
await wait(50);
await ev(() => h.classList.add('on'));
await wait(50);
console.log('all mid: ' + await ev(() => { const s = getComputedStyle(h); return [s.width, s.backgroundColor, s.transform].join(' | '); }));
await wait(200);
console.log('all end: ' + await ev(() => { const s = getComputedStyle(h); return [s.width, s.backgroundColor, s.transform, log.sort().join()].join(' | '); }));

await page.setContent(PAGE.replace('transition:opacity 100ms linear', 'transition:opacity 100ms linear 100ms'));
await wait(50);
await ev(() => h.classList.add('on'));
await wait(50);
console.log('delay 50: ' + await state());
await wait(100);
console.log('delay 150: ' + await state());
await wait(150);
console.log('delay 300: ' + await state());

await page.setContent(PAGE.replace('transition:opacity 100ms linear', 'transition:width 100ms linear'));
await wait(50);
await ev(() => h.classList.add('on'));
await wait(16);
console.log('other property: ' + await state());

await browser.close();
