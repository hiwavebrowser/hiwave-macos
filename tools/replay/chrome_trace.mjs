#!/usr/bin/env node
// Load one URL through the replay proxy in Chrome and write the page-trace
// schema HiWave writes under HIWAVE_TRACE_JSON. See tools/replay/README.md.

import { createRequire } from "node:module";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);
const { chromium } = require(resolve(here, "../parity_oracle/node_modules/playwright-core"));

const probeSource = readFileSync(resolve(here, "probe.js"), "utf8");

export function maxProfileDepth(profile) {
  const nodes = (profile && profile.nodes) || [];
  const byId = new Map(nodes.map((node) => [node.id, node]));
  const memo = new Map();
  function depth(id) {
    if (memo.has(id)) return memo.get(id);
    const node = byId.get(id);
    if (!node || node.parent == null) {
      memo.set(id, 0);
      return 0;
    }
    const value = depth(node.parent) + 1;
    memo.set(id, value);
    return value;
  }
  let max = 0;
  for (const node of nodes) max = Math.max(max, depth(node.id));
  return max;
}

function skipScript(url) {
  if (!url) return false;
  if (url.endsWith("replay-probe.js")) return true;
  if (url.startsWith("chrome-extension:") || url.startsWith("devtools:")) return true;
  return false;
}

export function scriptsFromParsed(parsed, networkByUrl, started, pageUrl = "") {
  const scripts = [];
  let index = 0;
  let order = 0;
  for (const event of parsed) {
    const raw = event.url || "";
    if (skipScript(raw) || event.isDefault === false) continue;
    // An external script has its own http(s) URL. An inline script is reported
    // with an empty URL or with the document URL; HiWave calls those inline#n
    // in document order, counting external scripts too.
    const external = (raw.startsWith("http://") || raw.startsWith("https://")) && raw !== pageUrl;
    index += 1;
    const url = external ? raw : `inline#${index}`;
    const t = Math.max(0, Math.round((event.at || started) - started));
    scripts.push({ url, order, event: "execute", t_ms: t });
    if (external) {
      const net = networkByUrl.get(raw);
      if (net && (net.failed || net.status >= 400)) {
        scripts.push({ url, order, event: "error", t_ms: t });
      } else if (net && net.status > 0 && net.status < 400) {
        scripts.push({ url, order, event: "load", t_ms: t });
      }
    }
    order += 1;
  }
  return scripts;
}

function arg(name, fallback) {
  const index = process.argv.indexOf(name);
  if (index === -1 || index + 1 >= process.argv.length) return fallback;
  return process.argv[index + 1];
}

function selfTest() {
  const depth = maxProfileDepth({
    nodes: [
      { id: 1, parent: undefined },
      { id: 2, parent: 1 },
      { id: 3, parent: 2 },
      { id: 4, parent: 2 },
    ],
  });
  if (depth !== 2) {
    console.error(`profile depth ${depth}`);
    process.exit(1);
  }
  const scripts = scriptsFromParsed(
    [
      { url: "replay-probe.js", at: 10 },
      { url: "https://example.test/app.js", at: 20 },
      { url: "", at: 30 },
    ],
    new Map([["https://example.test/app.js", { status: 200, failed: false }]]),
    0,
  );
  if (scripts[0].event !== "execute" || scripts[1].event !== "load" || scripts[2].url !== "inline#2") {
    console.error(JSON.stringify(scripts));
    process.exit(1);
  }
  console.log("chrome_trace self-test ok");
}

async function main() {
  if (process.argv.includes("--self-test")) {
    selfTest();
    return;
  }
  const replay = arg("--replay", "");
  const url = arg("--url", "");
  const out = arg("--out", "");
  if (!replay || !url || !out) {
    console.error("usage: chrome_trace.mjs --replay http://127.0.0.1:PORT --url https://example.test/ --out trace.json");
    process.exit(2);
  }
  const browser = await chromium.launch({
    executablePath: process.env.CHROME_PATH || "/usr/bin/google-chrome-stable",
    headless: true,
    proxy: { server: replay },
    args: [
      "--ignore-certificate-errors",
      "--disable-quic",
      "--disable-http2",
      "--disable-gpu",
    ],
  });
  const context = await browser.newContext({ ignoreHTTPSErrors: true });
  const page = await context.newPage();
  await page.addInitScript({ content: probeSource });
  const cdp = await context.newCDPSession(page);
  await cdp.send("Network.enable");
  await cdp.send("Debugger.enable");
  let profileOn = false;
  try {
    await cdp.send("Profiler.enable");
    await cdp.send("Profiler.setSamplingInterval", { interval: 100 });
    await cdp.send("Profiler.start");
    profileOn = true;
  } catch (err) {
    console.error(`profiler unavailable: ${err}`);
  }
  const started = Date.now();
  const parsed = [];
  const networkByUrl = new Map();
  cdp.on("Debugger.scriptParsed", (event) => {
    const aux = event.executionContextAuxData || {};
    parsed.push({
      url: event.url || "",
      at: Date.now(),
      length: event.length || 0,
      scriptId: event.scriptId,
      isDefault: aux.isDefault === true,
      contextType: aux.type || "",
      embedder: event.embedderName || "",
    });
    if (process.env.REPLAY_DEBUG === "1") {
      console.error(JSON.stringify({
        url: event.url || "",
        length: event.length,
        isDefault: aux.isDefault,
        type: aux.type,
        embedder: event.embedderName || "",
        startLine: event.startLine,
      }));
    }
  });
  cdp.on("Network.responseReceived", (event) => {
    const mime = String(event.response.mimeType || "");
    if (event.type === "Script" || mime.includes("javascript")) {
      networkByUrl.set(event.response.url, { status: event.response.status, failed: false });
    }
  });
  cdp.on("Network.loadingFailed", (event) => {
    for (const [key, row] of networkByUrl) {
      if (event.requestId && row.requestId === event.requestId) row.failed = true;
      void key;
    }
  });
  const failedById = new Map();
  cdp.on("Network.responseReceived", (event) => {
    failedById.set(event.requestId, event.response.url);
  });
  cdp.on("Network.loadingFailed", (event) => {
    const url = failedById.get(event.requestId);
    if (url && networkByUrl.has(url)) networkByUrl.get(url).failed = true;
  });
  await page.goto(url, { waitUntil: "load", timeout: 30000 });
  // Chrome keeps parsing its own helpers after load. Keep only the scripts
  // that existed when load fired, and drop the injected probe.
  const seen = parsed.slice();
  const pageEvents = [];
  for (const event of seen) {
    if (event.isDefault === false) continue;
    if (!event.url && event.scriptId) {
      try {
        const { scriptSource } = await cdp.send("Debugger.getScriptSource", { scriptId: event.scriptId });
        if (String(scriptSource).includes("__pageTrace")) continue;
      } catch {
        // Keep it; a missing source is still a page script if it was default.
      }
    }
    pageEvents.push(event);
  }
  const probe = await page.evaluate(() => window.__pageTrace || null);
  let profileDepth = 0;
  if (profileOn) {
    try {
      const { profile } = await cdp.send("Profiler.stop");
      profileDepth = maxProfileDepth(profile);
    } catch (err) {
      console.error(`profiler stop failed: ${err}`);
    }
  }
  await browser.close();
  const trace = {
    browser: "chrome",
    url,
    ready_state: (probe && probe.ready_state) || [],
    dcl_ms: probe ? probe.dcl_ms : null,
    load_ms: probe ? probe.load_ms : null,
    scripts: scriptsFromParsed(pageEvents, networkByUrl, started, url),
    console: (probe && probe.console) || [],
    mutations: (probe && probe.mutations) || [],
    max_js_stack: Math.max(profileDepth, (probe && probe.max_js_stack) || 0),
    shield: "off (stock Chrome)",
  };
  writeFileSync(out, JSON.stringify(trace, null, 2));
  console.log(`CHROME_TRACE ${out}`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
