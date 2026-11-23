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

function attrs(node) {
  const out = {};
  const list = node.attributes || [];
  if (!Array.isArray(list)) return list;
  for (let i = 0; i < list.length; i += 2) out[list[i]] = list[i + 1] || "";
  return out;
}

function styleMap(computed) {
  const out = {};
  for (const item of computed || []) out[item.name] = item.value;
  return out;
}

function boxFromModel(model) {
  if (!model || !model.border) return "no box";
  const q = model.border;
  const xs = [q[0], q[2], q[4], q[6]];
  const ys = [q[1], q[3], q[5], q[7]];
  const x = Math.min(...xs);
  const y = Math.min(...ys);
  return {
    x: Math.round(x * 100) / 100,
    y: Math.round(y * 100) / 100,
    width: Math.round((Math.max(...xs) - x) * 100) / 100,
    height: Math.round((Math.max(...ys) - y) * 100) / 100,
  };
}

function customState(raw) {
  if (raw === "custom" || raw === "precustomized") return "defined";
  if (raw === "undefined" || raw === "failed" || raw === "uncustomized") return raw;
  return raw || "uncustomized";
}

function boxesIntersect(a, b) {
  return a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;
}

function overflowClips(overflow) {
  if (!overflow || overflow === "visible") return false;
  return String(overflow).split(/\s+/).some((part) => part && part !== "visible");
}

function chromeSkip(node, style, box, info, lookup) {
  const parent = node.parentNode;
  if (parent && parent.shadowRoots && parent.shadowRoots.length) {
    const assigned = info && info.slot ? info.slot.assigned_to : null;
    if (assigned == null) return "unslotted light DOM";
  }
  const display = style.display || "";
  const cv = style["content-visibility"] || "visible";
  if (display === "none") return "display:none";
  if (cv === "hidden") return "content-visibility";
  if (box === "no box") return "no box";
  if (box && box.width <= 0 && box.height <= 0) return "zero size";
  if (lookup && box && box !== "no box") {
    let cursor = parent;
    while (cursor) {
      const ancestor = lookup(cursor) || {};
      const ancestorStyle = ancestor.style || {};
      if (overflowClips(ancestorStyle.overflow) && ancestor.box && ancestor.box !== "no box") {
        if (!boxesIntersect(box, ancestor.box)) return "clipped";
      }
      cursor = cursor.parentNode || null;
    }
  }
  return null;
}

function chromePaints(style, box) {
  if (box === "no box" || !box) return false;
  if ((style.display || "") === "none") return false;
  if ((style.visibility || "") === "hidden") return false;
  const opacity = Number(style.opacity);
  if (Number.isFinite(opacity) && opacity <= 0) return false;
  return true;
}

// `lookup(node)` answers { style, box, adopted, assignedTo, assignedCount }
// for one CDP node. The tree is the path from main#main-content to the first
// three shreddit-post elements, plus their direct children. Chrome has no
// display list: `in_display_list` means the box model exists and the element
// is not display:none, visibility:hidden, or opacity:0.
export function assembleRender(main, lookup, tMs = 0) {
  if (!main) {
    return {
      t_ms: tMs,
      when: "quiet",
      display_list_items_in_main: 0,
      main: null,
      note: "no main#main-content",
    };
  }
  const posts = [];
  function walk(node) {
    if (posts.length >= 3) return;
    if (String(node.nodeName || "").toLowerCase() === "shreddit-post") posts.push(node);
    for (const root of node.shadowRoots || []) walk(root);
    for (const child of node.children || []) {
      if (child.nodeType === 1 || child.nodeName) walk(child);
    }
  }
  walk(main);
  const keep = new Set();
  function idOf(node) {
    return node.backendNodeId || node.nodeId;
  }
  keep.add(idOf(main));
  for (const post of posts) {
    let cursor = post;
    const guard = new Set();
    while (cursor && !guard.has(idOf(cursor))) {
      guard.add(idOf(cursor));
      keep.add(idOf(cursor));
      if (cursor === main) break;
      cursor = cursor.parentNode || null;
    }
  }
  let painted = 0;
  function element(node, descend) {
    const info = lookup(node) || {};
    const style = info.style || {};
    const box = info.box || "no box";
    const paints = chromePaints(style, box);
    if (paints) painted += 1;
    const attributes = attrs(node);
    const name = String(node.nodeName || "").toLowerCase();
    const isPost = posts.includes(node);
    const children = [];
    if (descend && keep.has(idOf(node))) {
      const kids = [];
      for (const root of node.shadowRoots || []) kids.push(...(root.children || [root]));
      for (const child of node.children || []) kids.push(child);
      const seen = new Set();
      for (const child of kids) {
        if (!child || seen.has(idOf(child))) continue;
        if (child.nodeType && child.nodeType !== 1) continue;
        seen.add(idOf(child));
        const childDescend = keep.has(idOf(child)) && !isPost;
        children.push(element(child, childDescend));
      }
    }
    const shadow = (node.shadowRoots && node.shadowRoots[0]) || null;
    return {
      tag: name,
      id: attributes.id || "",
      class: attributes.class || "",
      custom_element: customState(node.customElementState),
      hidden: Object.prototype.hasOwnProperty.call(attributes, "hidden"),
      shadow: shadow
        ? {
            mode: shadow.shadowRootType || "open",
            child_count: (shadow.children || []).length,
            adopted_stylesheets: info.adopted || 0,
          }
        : null,
      slot: info.slot || null,
      style: {
        display: style.display || null,
        visibility: style.visibility || null,
        opacity: style.opacity == null ? null : Number(style.opacity),
        "content-visibility": style["content-visibility"] || "visible",
        position: style.position || null,
        height: style.height || null,
        overflow: style.overflow || null,
      },
      box,
      in_display_list: paints,
      skip: chromeSkip(node, style, box, info, lookup),
      children,
    };
  }
  const tree = element(main, true);
  return {
    t_ms: tMs,
    when: "quiet",
    display_list_items_in_main: painted,
    main: tree,
  };
}

export function parserFromResources(entries) {
  const out = [];
  for (const entry of entries || []) {
    const kind = String(entry.initiatorType || "");
    let blocked = "";
    if (kind === "link" || kind === "css") blocked = "stylesheet";
    else if (kind === "script") blocked = "script fetch";
    else continue;
    const name = String(entry.name || "");
    if (!name || name.includes("replay-probe")) continue;
    out.push({
      blocked_on: blocked,
      ms: Math.max(0, Math.round(Number(entry.duration) || 0)),
      target: name,
    });
  }
  return out;
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
  const main = {
    nodeName: "MAIN",
    nodeId: 1,
    attributes: ["id", "main-content"],
    customElementState: "uncustomized",
    children: [{
      nodeName: "SHREDDIT-POST",
      nodeId: 2,
      parentNode: null,
      attributes: ["class", "post"],
      customElementState: "custom",
      children: [{ nodeName: "DIV", nodeId: 3, attributes: [], children: [] }],
    }],
  };
  main.children[0].parentNode = main;
  main.children[0].children[0].parentNode = main.children[0];
  const render = assembleRender(main, (node) => {
    if (node.nodeId === 3) return { style: { display: "none", visibility: "visible", opacity: "1" }, box: "no box" };
    return {
      style: { display: "block", visibility: "visible", opacity: "1", position: "static", height: "20px", overflow: "visible" },
      box: { x: 0, y: 0, width: 10, height: 10 },
    };
  });
  if (render.main.tag !== "main" || render.main.children[0].custom_element !== "defined") {
    console.error(JSON.stringify(render));
    process.exit(1);
  }
  if (render.main.children[0].children[0].skip !== "display:none") {
    console.error(JSON.stringify(render));
    process.exit(1);
  }
  const parser = parserFromResources([
    { name: "https://example.test/app.css", initiatorType: "link", duration: 12.2 },
    { name: "https://example.test/app.js", initiatorType: "script", duration: 4 },
    { name: "https://example.test/pixel.png", initiatorType: "img", duration: 9 },
  ]);
  if (parser.length !== 2 || parser[0].blocked_on !== "stylesheet" || parser[1].blocked_on !== "script fetch") {
    console.error(JSON.stringify(parser));
    process.exit(1);
  }
  const host = {
    nodeName: "MAIN",
    nodeId: 1,
    attributes: ["id", "main-content"],
    shadowRoots: [{ shadowRootType: "open", children: [] }],
    children: [{ nodeName: "SPAN", nodeId: 11, attributes: [], children: [] }],
  };
  host.children[0].parentNode = host;
  const unslotted = assembleRender(host, () => ({
    style: { display: "block", visibility: "visible", opacity: "1", overflow: "visible" },
    box: { x: 0, y: 0, width: 10, height: 10 },
  }));
  if (unslotted.main.children[0].skip !== "unslotted light DOM") {
    console.error(JSON.stringify(unslotted));
    process.exit(1);
  }
  console.log("chrome_trace self-test ok");
}

function findNamed(node, pred, parent) {
  if (!node) return null;
  node.parentNode = parent || null;
  if (pred(node)) return node;
  for (const root of node.shadowRoots || []) {
    const found = findNamed(root, pred, node);
    if (found) return found;
  }
  for (const child of node.children || []) {
    const found = findNamed(child, pred, node);
    if (found) return found;
  }
  return null;
}

function cappedNodes(main) {
  const posts = [];
  function walk(node) {
    if (!node || posts.length >= 3) return;
    if (String(node.nodeName || "").toLowerCase() === "shreddit-post") posts.push(node);
    for (const root of node.shadowRoots || []) {
      for (const child of root.children || []) walk(child);
    }
    for (const child of node.children || []) walk(child);
  }
  walk(main);
  const keep = new Set([main]);
  for (const post of posts) {
    let cursor = post;
    while (cursor) {
      keep.add(cursor);
      if (cursor === main) break;
      cursor = cursor.parentNode;
    }
  }
  const nodes = [];
  function take(node, descend) {
    if (!node || !keep.has(node)) return;
    nodes.push(node);
    if (!descend) return;
    const isPost = posts.includes(node);
    const kids = [];
    for (const root of node.shadowRoots || []) kids.push(...(root.children || []));
    for (const child of node.children || []) kids.push(child);
    for (const child of kids) take(child, keep.has(child) && !isPost);
  }
  take(main, true);
  return nodes;
}

async function describeForRender(cdp, node) {
  let customElementState = node.customElementState;
  try {
    const described = await cdp.send("DOM.describeNode", { nodeId: node.nodeId });
    customElementState = (described.node && described.node.customElementState) || customElementState;
    if (described.node && described.node.shadowRoots && !node.shadowRoots) {
      node.shadowRoots = described.node.shadowRoots;
    }
  } catch {
    // describeNode is optional; getDocument already has the tree.
  }
  node.customElementState = customElementState;
  let computed = [];
  try {
    const styled = await cdp.send("CSS.getComputedStyleForNode", { nodeId: node.nodeId });
    computed = styled.computedStyle || [];
  } catch {
    computed = [];
  }
  let box = "no box";
  try {
    const model = await cdp.send("DOM.getBoxModel", { nodeId: node.nodeId });
    box = boxFromModel(model.model);
  } catch {
    box = "no box";
  }
  let adopted = 0;
  let slot = null;
  try {
    const { object } = await cdp.send("DOM.resolveNode", { nodeId: node.nodeId });
    const called = await cdp.send("Runtime.callFunctionOn", {
      objectId: object.objectId,
      returnByValue: true,
      functionDeclaration: `function() {
        var adopted = 0;
        try { adopted = this.adoptedStyleSheets ? this.adoptedStyleSheets.length : 0; } catch (e) {}
        var assigned = null;
        try { assigned = this.assignedSlot ? (this.assignedSlot.name || "") : null; } catch (e) {}
        var count = null;
        try { if (this.localName === "slot" && this.assignedNodes) count = this.assignedNodes().length; } catch (e) {}
        var name = null;
        try { if (this.getAttribute) name = this.getAttribute("slot"); } catch (e) {}
        return { adopted: adopted, assignedTo: assigned, assignedCount: count, slotName: name };
      }`,
    });
    const value = (called.result && called.result.value) || {};
    adopted = value.adopted || 0;
    if (value.slotName != null || value.assignedTo != null || value.assignedCount != null) {
      slot = { name: value.slotName, assigned_to: value.assignedTo, assigned_count: value.assignedCount };
    }
  } catch {
    // Slot assignment still comes back null. The box and style above stand.
  }
  return { style: styleMap(computed), box, adopted, slot };
}

async function collectChromeRender(cdp, tMs) {
  await cdp.send("DOM.enable");
  await cdp.send("CSS.enable");
  const { root } = await cdp.send("DOM.getDocument", { depth: -1, pierce: true });
  const main = findNamed(root, (node) => {
    if (String(node.nodeName || "").toLowerCase() !== "main") return false;
    return attrs(node).id === "main-content";
  }, null);
  if (!main) return assembleRender(null, () => ({}), tMs);
  const info = new Map();
  for (const node of cappedNodes(main)) {
    if (!node.nodeId || info.has(node.nodeId)) continue;
    info.set(node.nodeId, await describeForRender(cdp, node));
  }
  return assembleRender(main, (node) => info.get(node.nodeId) || {}, tMs);
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
  // CSS/DOM/Runtime calls below parse helper scripts in the page. Those are
  // not the document's scripts; snapshot before them so they do not take
  // the next inline#N.
  const parsedAtLoad = parsed.slice();
  let render = null;
  let parser = [];
  try {
    render = await collectChromeRender(cdp, Date.now() - started);
  } catch (err) {
    console.error(`render dump failed: ${err}`);
  }
  try {
    const entries = await page.evaluate(() =>
      performance.getEntriesByType("resource").map((entry) => ({
        name: entry.name,
        initiatorType: entry.initiatorType,
        duration: entry.duration,
      })),
    );
    parser = parserFromResources(entries);
  } catch (err) {
    console.error(`parser timeline failed: ${err}`);
  }
  // Chrome keeps parsing its own helpers after load. Keep only the scripts
  // that existed when load fired, and drop the injected probe.
  const seen = parsedAtLoad;
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
    parser,
    render,
  };
  writeFileSync(out, JSON.stringify(trace, null, 2));
  console.log(`CHROME_TRACE ${out}`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
