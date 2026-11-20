#!/usr/bin/env node
/**
 * report_chart_resize_log.mjs - Chrome's answers for REPORT-CHART-0x0 (E10b):
 * when a ResizeObserver callback runs relative to the animation frame
 * callbacks already asked for, and what the app's Chart.js 4.4 does with
 * report.html's shape (charts built while `#content` is display:none,
 * shown in the same task, default animation on).
 *
 *   PARITY_CHROME_PATH=<pinned Chrome for Testing> node report_chart_resize_log.mjs \
 *     > docs/diagnostics/2026-11-19/report_chart_resize_cft148.json
 *
 * The pages are the ones in rustkit-engine `load_geometry_read_tests.rs`
 * (`a_resize_observation_waits_for_the_frame_already_asked_for`,
 * `real_chart_js_built_hidden_resizes_both_charts_when_shown`); keep them equal.
 */
import { chromium } from 'playwright';
import { readFileSync } from 'fs';
import { dirname, join } from 'path';
import { fileURLToPath } from 'url';
import { getDeterministicLaunchOptions } from './deterministic.mjs';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const CHART = readFileSync(join(ROOT, 'crates/hiwave-app/src/ui/chart.umd.min.js'), 'utf8');

// A frame loop asked for before the observed box changes (Chart.js's animator),
// then a ResizeObserver callback that asks for a frame of its own (Chart.js's
// `throttled` resize).
export const ORDER_PAGE = "<!doctype html><html><head><style>body{margin:0} #cc{height:300px}</style></head><body>" +
  "<div id=content style='display:none'><div id=cc></div></div><div id=out>x</div><script>" +
  "var log=[];function L(s){log.push(s);document.getElementById('out').textContent='R:'+log.join(' ');}" +
  "var n=0;function A(){n++;L('A'+n);if(n<3)requestAnimationFrame(A);}" +
  "document.addEventListener('DOMContentLoaded',function(){Promise.resolve().then(function(){" +
  "requestAnimationFrame(A);" +
  "new ResizeObserver(function(es){var r=es[0].contentRect;L('ro='+r.width+'x'+r.height);" +
  "requestAnimationFrame(function(){L('B');});}).observe(document.getElementById('cc'));" +
  "document.getElementById('content').style.display='block';});});" +
  "</script></body></html>";

export function chartPage(chart) {
  return "<!doctype html><html><head><style>body{margin:0;padding:20px}" +
    ".chart-container{position:relative;height:300px}</style></head><body>" +
    "<div id=content style='display:none'><div class=chart-container><canvas id=activityChart></canvas></div>" +
    "<div class=chart-container><canvas id=protectionChart></canvas></div></div>" +
    "<div id=out>x</div><script>" + chart + "</script><script>" +
    // report.html's default period ('today'): one label, its colours (a dataset with colours of
    // its own has no colour animation left to run once the chart is drawn at 0x0).
    "function mk(id,type){return new Chart(document.getElementById(id).getContext('2d'),{type:type," +
    "data:{labels:['Today'],datasets:[{data:[42],borderColor:'#06b6d4',backgroundColor:'rgba(6,182,212,0.1)',fill:true,tension:0.4}," +
    "{data:[12],borderColor:'#10b981',backgroundColor:'rgba(16,185,129,0.1)',fill:true,tension:0.4}]}," +
    "options:{responsive:true,maintainAspectRatio:false,scales:{y:{beginAtZero:true}}}});}" +
    "function sizes(){return ['activityChart','protectionChart'].map(function(i){var c=document.getElementById(i);" +
    "return i+'='+c.width+'x'+c.height;}).join(' ');}" +
    "document.addEventListener('DOMContentLoaded',function(){Promise.resolve().then(function(){" +
    "mk('activityChart','line');mk('protectionChart','bar');" +
    "document.getElementById('content').style.display='block';" +
    "document.getElementById('out').textContent='R:built '+sizes();" +
    "setTimeout(function(){document.getElementById('out').textContent='R:later '+sizes();},300);});});" +
    "</script></body></html>";
}

const browser = await chromium.launch({ ...getDeterministicLaunchOptions(), headless: true });
const out = { version: browser.version(), viewport: '800x600' };
for (const [name, html] of [['order', ORDER_PAGE], ['chart', chartPage(CHART)]]) {
  const page = await browser.newPage({ viewport: { width: 800, height: 600 } });
  await page.setContent(html);
  await page.waitForTimeout(600);
  out[name] = await page.evaluate(() => document.getElementById('out').textContent);
  await page.close();
}
console.log(JSON.stringify(out, null, 1));
await browser.close();
