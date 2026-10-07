#!/usr/bin/env node
// summarize_technique_census.mjs — technique_census_<date>.json -> TECHNIQUE_CENSUS_<date>.md prevalence tables.
// Usage: node docs/census/tools/summarize_technique_census.mjs docs/census/technique_census_2026-10-07.json [out.md] [out2.md ...]
import { readFileSync, writeFileSync, mkdirSync } from 'fs';
import { dirname } from 'path';
const [inp, ...outArgs] = process.argv.slice(2);
const { meta, results } = JSON.parse(readFileSync(inp, 'utf8'));
const defaultOut = inp.replace(/technique_census_(.*)\.json$/, 'TECHNIQUE_CENSUS_$1.md');
const outs = outArgs.length ? outArgs : [defaultOut];
const ok = results.filter(r => r.status === 'OK' || r.status === 'BOTWALL');
const loaded = results.filter(r => r.status === 'OK');
const N = loaded.length;
const pct = (n, den = N) => `${n}/${den} (${den ? Math.round(100 * n / den) : 0}%)`;
const row = (name, pred, note = '', pool = loaded) => {
  const s = pool.filter(r => r.data && pred(r.data)).map(r => r.id);
  return { name, n: s.length, s, note };
};
const table = (title, rows, den = N) => {
  rows.sort((a, b) => b.n - a.n);
  return `### ${title}\n\n| Technique | Sites | Site ids |\n|---|:---:|---|\n` +
    rows.map(r => `| ${r.name}${r.note ? ` (${r.note})` : ''} | ${pct(r.n, den)} | ${r.s.slice(0, 40).join(', ')}${r.s.length > 40 ? ', …' : ''} |`).join('\n') + '\n';
};
const I = (d) => d.icons, L = (d) => d.lazy, S = (d) => d.stacking, O = (d) => d.objectFit;
const B = (d) => d.bundles || { count: 0, total_bytes: 0, over_1mb_count: 0, over_1mb: [] };
const F = (d) => d.frameworks || { detected: [] };
const W = (d) => d.botWall || { vendors: [] };

const icons = [
  row('Inline `<svg>` (any)', d => I(d).inline_svg.count > 0),
  row('Inline `<svg>` (icon-sized ≤64px)', d => I(d).inline_svg.icon_sized > 0),
  row('`<svg><use href="#id">` local', d => I(d).svg_use_local.count > 0),
  row('`<svg><use href="file.svg#id">` external', d => I(d).svg_use_external.count > 0),
  row('`<img>` SVG', d => I(d).img_svg.count > 0),
  row('CSS `background-image` SVG', d => I(d).css_bg_svg.count > 0),
  row('CSS `mask-image` url()', d => I(d).mask_image_url.count > 0, 'icon masks'),
  row('CSS `mask-image` gradient', d => I(d).mask_image_gradient.count > 0, 'fades, not icons'),
  row('Icon font (PUA glyphs)', d => I(d).icon_font_pua.count > 0),
];
const lazy = [
  row('`loading=lazy` on `<img>`', d => L(d).native_loading_lazy_img > 0),
  row('`loading=lazy` on `<iframe>`', d => L(d).native_loading_lazy_iframe > 0),
  row('`data-src`/`data-srcset`-style deferred attrs', d => L(d).data_src_attr > 0),
  row('`lazy*` class names', d => L(d).lazy_class > 0),
  row('Placeholder `src` (data: gif/png/svg)', d => L(d).placeholder_src > 0),
  row('IntersectionObserver used', d => (L(d).intersection_observer && (L(d).intersection_observer.observe || 0)) > 0),
  row('`srcset` on `<img>`', d => L(d).srcset_img > 0),
  row('`<picture>`', d => L(d).picture_el > 0),
  row('`decoding=async`', d => L(d).decoding_async > 0),
  row('`fetchpriority=high`', d => L(d).fetchpriority_high > 0),
  row('`content-visibility:auto`', d => L(d).content_visibility_auto > 0),
  row('`<noscript><img>` fallback', d => L(d).noscript_img > 0),
];
const reasons = new Set(); loaded.forEach(r => Object.keys(S(r.data).by_reason || {}).forEach(k => reasons.add(k)));
const stacking = [...reasons].map(k => row(`\`${k}\``, d => (S(d).by_reason[k] || 0) > 0));
const posTypes = ['fixed', 'sticky', 'absolute', 'relative'];
const stackingPos = posTypes.map(p => row(`\`position:${p}\` in region`, d => (S(d).by_position && S(d).by_position[p] || 0) > 0));
const fits = new Set(); loaded.forEach(r => Object.keys(O(r.data).by_value || {}).forEach(k => fits.add(k)));
const ofit = [...fits].map(k => row(`\`object-fit: ${k}\``, d => (O(d).by_value[k] || 0) > 0));
ofit.push(row('non-default `object-position`', d => O(d).non_default_position > 0));

const fwNames = new Set(); loaded.forEach(r => (F(r.data).detected || []).forEach(n => fwNames.add(n)));
const frameworks = [...fwNames].map(n => row(n, d => (F(d).detected || []).includes(n)));

const bundles = [
  row('Any JS script resource', d => B(d).count > 0),
  row('≥1 JS bundle >1MB', d => B(d).over_1mb_count > 0),
  row('≥2 JS bundles >1MB', d => B(d).over_1mb_count >= 2),
  row('Total JS ≥5MB', d => (B(d).total_bytes || 0) >= 5 * 1024 * 1024),
  row('Total JS ≥10MB', d => (B(d).total_bytes || 0) >= 10 * 1024 * 1024),
];
const heavy = loaded
  .filter(r => B(r.data).over_1mb_count > 0)
  .map(r => ({
    id: r.id,
    over: B(r.data).over_1mb_count,
    total_mb: B(r.data).total_mb,
    top: (B(r.data).over_1mb || []).slice(0, 3).map(b => `${b.mb}MB`).join(', '),
  }))
  .sort((a, b) => b.over - a.over || b.total_mb - a.total_mb);

const allForWall = results.filter(r => r.data);
const wallVendors = new Set();
allForWall.forEach(r => (W(r.data).vendors || []).forEach(v => wallVendors.add(v)));
const botwalls = [...wallVendors].map(v => row(v, d => (W(d).vendors || []).includes(v), '', allForWall));
botwalls.push(row('HTTP 403', d => W(d).http_status === 403 || false, '', results.filter(r => r.http_status != null || r.data)));
const wallSites = results.filter(r => r.status === 'BOTWALL' || (r.data && W(r.data).is_challenge) || r.http_status === 403);
// Walls by vendor: only sites that actually walled us (403 / challenge), attributed to the vendor seen on that response.
const wallsByVendorMap = new Map();
wallSites.forEach(r => {
  const vs = (W(r.data || {}).vendors || []);
  (vs.length ? vs : ['unknown']).forEach(v => { if (!wallsByVendorMap.has(v)) wallsByVendorMap.set(v, []); wallsByVendorMap.get(v).push(r.id); });
});
const wallsByVendor = [...wallsByVendorMap].map(([name, s]) => ({ name, n: s.length, s, note: '' }));
const okButChallenged = wallSites.filter(r => r.status === 'OK').map(r => `${r.id} (http=${r.http_status})`);

const notrun = results.filter(r => r.status !== 'OK' && r.status !== 'BOTWALL');

const heavyTable = heavy.length
  ? `### Sites with JS bundles >1MB\n\n| Site | Bundles >1MB | Total JS MB | Top bundle sizes |\n|---|:---:|:---:|---|\n` +
    heavy.map(h => `| ${h.id} | ${h.over} | ${h.total_mb} | ${h.top} |`).join('\n') + '\n'
  : '### Sites with JS bundles >1MB\n\nNone observed.\n';

const wallDetail = wallSites.length
  ? wallSites.map(r => `- **${r.id}**: http=${r.http_status} vendors=${(W(r.data || {}).vendors || []).join(',') || 'unknown'} title=${(r.data && r.data.title || '').slice(0, 60)}`).join('\n')
  : 'None.';

const md = `# Technique census — ${meta.date}

Universe: \`${meta.universe}\` (${meta.n_sites} sites; "top 100" = top80 this week per ZeuzGb). Chromium ${meta.chromium} (\`${meta.chrome_source || meta.executablePath}\`), viewport ${meta.viewport.width}x${meta.viewport.height}, logged-out, one load per site, first viewport + full DOM after networkidle (≤8s) + 1.5s settle. Run ${meta.started_utc} → ${meta.finished_utc}.

Loaded OK: **${N}/${meta.n_sites}**. Bot-wall / 403: **${wallSites.length}**. Failed: **${notrun.length}**. Counts below are sites using the technique at least once, out of the ${N} that loaded OK (bot-wall section uses all observed responses). Generated by \`docs/census/tools/run_technique_census.mjs\` + \`summarize_technique_census.mjs\`; raw per-site data with example selectors in \`${inp.split('/').pop()}\`.

Icon taxonomy aligned with Pollux top-20 diagnostics (PR #565): inline SVG, \`<svg><use>\`, CSS background-image SVG, CSS mask-image, \`<img>\` SVG, icon-font PUA.

## Icons
${table('Icon technique prevalence', icons)}
## Image lazy-load / sizing patterns
${table('Lazy-load / image-loading pattern prevalence', lazy)}
## object-fit
${table('object-fit prevalence', ofit)}
## Stacking-context creators in header / nav / menu / overlay regions
Region = \`header\`, \`nav\`, \`dialog\`, ARIA banner/navigation/menu/menubar/dialog/haspopup/modal, and elements whose id/class names header/nav/menu/dropdown/flyout/masthead/banner/overlay/modal/popover/drawer, plus descendants.

${table('Stacking-context creator prevalence', stacking)}
${table('Position types in region', stackingPos)}
## Frameworks
${table('Framework / platform prevalence', frameworks)}
## JS bundles
${table('JS bundle size prevalence', bundles)}
${heavyTable}
## Bot walls (403 / challenge)
${table('Walls by vendor (sites that walled us)', wallsByVendor, wallSites.length)}
Denominator is the ${wallSites.length} walled sites (HTTP 403 or challenge page).${okButChallenged.length ? ` Note: ${okButChallenged.join(', ')} served a challenge page but is still counted in Loaded OK above.` : ''}

${table('Edge / CDN vendor seen (any observed response; not walls)', botwalls, allForWall.length || results.length)}
Vendor fingerprints seen on any response, including sites that loaded fine behind that vendor. Use the table above for wall counts.

### Challenge / 403 sites
${wallDetail}

## NOT RUN / failed
${notrun.length ? notrun.map(r => `- ${r.id}: ${r.reason}`).join('\n') : 'None.'}
`;

for (const out of outs) {
  mkdirSync(dirname(out), { recursive: true });
  writeFileSync(out, md);
  console.error(`wrote ${out}`);
}
