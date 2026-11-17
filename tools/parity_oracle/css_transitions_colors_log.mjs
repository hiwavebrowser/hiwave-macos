// What the pinned Chrome reports for the colour transitions of group A
// (outline-color, text-decoration-color, fill, stroke): the value at the
// midpoint of a 100ms linear transition, read at a fixed currentTime
// (the transition is paused and seeked, no timers), and whether a
// transition starts at all.
// usage: PARITY_CHROME_PATH=... node tools/parity_oracle/css_transitions_colors_log.mjs
import { chromium } from 'playwright';
import { getDeterministicLaunchOptions } from './deterministic.mjs';

if (!process.env.PARITY_CHROME_PATH) {
  throw new Error('PARITY_CHROME_PATH must name the pinned Chrome for Testing');
}

const browser = await chromium.launch(getDeterministicLaunchOptions());
const page = await browser.newPage({ viewport: { width: 800, height: 600 } });
console.log('chrome ' + browser.version());

// `css` styles #h; `on` is added to it; `read` is the property read back
// from `target` (#h or #s, the svg rect) at currentTime 50 of 100.
async function probe(label, css, read, target = 'h') {
  await page.setContent('<!doctype html><style>body{margin:0}' + css + '</style>' +
    '<div id=h>text <span id=c>child</span></div>' +
    '<svg width=50 height=50><rect id=s class=r width=50 height=50 /></svg>');
  await page.evaluate(() => document.body.offsetWidth);
  const out = await page.evaluate(([read, target]) => {
    const el = document.getElementById(target);
    el.classList.add('on');
    getComputedStyle(el).getPropertyValue(read);
    const anims = document.getAnimations();
    anims.forEach((a) => { a.pause(); a.currentTime = 50; });
    const names = anims.map((a) => a.transitionProperty).join(',');
    return [getComputedStyle(el).getPropertyValue(read),
      'child ' + getComputedStyle(document.getElementById('c')).getPropertyValue(read),
      'anims [' + names + ']'].join(' | ');
  }, [read, target]);
  console.log(label + ': ' + out);
}

const T = 'transition:all 100ms linear;';
await probe('outline-color rgb', `#h{outline:2px solid rgb(0,0,255);${T}}#h.on{outline-color:rgb(255,0,0)}`, 'outline-color');
await probe('outline-color currentcolor->red', `#h{color:rgb(0,0,255);outline:2px solid;${T}}#h.on{outline-color:rgb(255,0,0)}`, 'outline-color');
await probe('outline-color red->currentcolor', `#h{color:rgb(0,0,255);outline:2px solid rgb(255,0,0);${T}}#h.on{outline-color:currentcolor}`, 'outline-color');
await probe('outline-color currentcolor, color jumps', `#h{color:rgb(0,0,255);outline:2px solid;transition:outline-color 100ms linear}#h.on{color:rgb(255,0,0)}`, 'outline-color');
await probe('outline-color transparent->red', `#h{outline:2px solid transparent;${T}}#h.on{outline-color:rgb(255,0,0)}`, 'outline-color');
await probe('outline shorthand named', `#h{outline:2px solid rgb(0,0,255);transition:outline 100ms linear}#h.on{outline-color:rgb(255,0,0)}`, 'outline-color');
await probe('text-decoration-color rgb', `#h{text-decoration:underline rgb(0,0,255);${T}}#h.on{text-decoration-color:rgb(255,0,0)}`, 'text-decoration-color');
await probe('text-decoration-color transparent->red', `#h{text-decoration:underline transparent;${T}}#h.on{text-decoration-color:rgb(255,0,0)}`, 'text-decoration-color');
await probe('text-decoration shorthand named', `#h{text-decoration:underline rgb(0,0,255);transition:text-decoration 100ms linear}#h.on{text-decoration-color:rgb(255,0,0)}`, 'text-decoration-color');
await probe('fill rgb', `.r{fill:rgb(0,0,255);${T}}.r.on{fill:rgb(255,0,0)}`, 'fill', 's');
await probe('fill none->red', `.r{fill:none;${T}}.r.on{fill:rgb(255,0,0)}`, 'fill', 's');
await probe('stroke rgb', `.r{stroke:rgb(0,0,255);${T}}.r.on{stroke:rgb(255,0,0)}`, 'stroke', 's');
await probe('stroke none->red', `.r{stroke:none;${T}}.r.on{stroke:rgb(255,0,0)}`, 'stroke', 's');

await browser.close();
