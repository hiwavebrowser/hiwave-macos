#!/usr/bin/env node
/**
 * chrome_trace_cascade.mjs — Capture Chrome DevTools timeline trace for cascade measurement.
 *
 * Used by scripts/cascade_bench.py under Package Z2-M1.
 * Loads a URL (such as a local pinned snapshot) in headless Chromium at 1280x800,
 * collects the timeline trace, and writes raw trace JSON to the specified output file.
 *
 * Usage:
 *   node scripts/chrome_trace_cascade.mjs --url http://127.0.0.1:8080/wikipedia/index.html --out trace_wiki.json
 */

import { createRequire } from 'module';
import { writeFileSync, mkdirSync } from 'fs';
import { dirname, resolve, join } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(__dirname, '..');
const require = createRequire(process.env.PW_ROOT || join(REPO, 'tools/parity_oracle/package.json'));
const { chromium } = require('playwright');

// Parse CLI arguments
const argv = process.argv.slice(2);
let url = null;
let outPath = null;
let settleMs = 2000;
let timeoutMs = 45000;

for (let i = 0; i < argv.length; i++) {
  const arg = argv[i];
  if (arg === '--url') url = argv[++i];
  else if (arg === '--out') outPath = argv[++i];
  else if (arg === '--settle-ms') settleMs = parseInt(argv[++i], 10);
  else if (arg === '--timeout-ms') timeoutMs = parseInt(argv[++i], 10);
}

if (!url || !outPath) {
  console.error('Usage: node chrome_trace_cascade.mjs --url <url> --out <trace.json> [--settle-ms <ms>]');
  process.exit(2);
}

async function main() {
  const launchOptions = {
    headless: true,
    args: [
      '--disable-background-networking',
      '--disable-background-timer-throttling',
      '--disable-backgrounding-occluded-windows',
      '--disable-renderer-backgrounding',
      '--no-sandbox',
    ],
  };

  if (process.env.PARITY_CHROME_PATH) {
    launchOptions.executablePath = process.env.PARITY_CHROME_PATH;
  }

  const browser = await chromium.launch(launchOptions);
  const context = await browser.newContext({
    viewport: { width: 1280, height: 800 },
    deviceScaleFactor: 1,
  });
  const page = await context.newPage();

  // Start timeline tracing
  await browser.startTracing(page, {
    categories: [
      'devtools.timeline',
      'v8',
      'blink',
      'loading',
      'disabled-by-default-devtools.timeline',
      'blink.user_timing',
    ],
  });

  try {
    await page.goto(url, { waitUntil: 'load', timeout: timeoutMs });
  } catch (err) {
    console.warn(`Warning: navigation error for ${url}: ${err.message}`);
  }

  await page.waitForTimeout(settleMs);

  const traceBuffer = await browser.stopTracing();
  await context.close();
  await browser.close();

  mkdirSync(dirname(resolve(outPath)), { recursive: true });
  writeFileSync(outPath, traceBuffer);
  console.log(`Trace saved to ${outPath} (${traceBuffer.length} bytes)`);
}

main().catch(err => {
  console.error(`Fatal trace error: ${err.stack || err.message}`);
  process.exit(1);
});
