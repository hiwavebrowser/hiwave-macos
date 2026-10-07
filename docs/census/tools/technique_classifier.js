// technique_classifier.js — runs inside the page (page.evaluate). Pure DOM/CSSOM reads, no mutation.
// Returns per-site technique records for the TECHNIQUE CENSUS (icons, lazy-load, stacking contexts, object-fit).
() => {
  const MAX_EL = 20000;
  const all = Array.from(document.querySelectorAll('*')).slice(0, MAX_EL);
  const PUA = /[\uE000-\uF8FF]|[\u{F0000}-\u{FFFFD}]|[\u{100000}-\u{10FFFD}]/u;
  const isSvgUrl = (v) => /url\(\s*["']?(data:image\/svg\+xml|[^"')]*\.svg(\?|#|["')]|$))/i.test(v || '');
  const hasUrl = (v) => /url\(/i.test(v || '');
  const hasGrad = (v) => /gradient\(/i.test(v || '');
  const rectOf = (el) => { const r = el.getBoundingClientRect(); return { x: Math.round(r.x), y: Math.round(r.y), w: Math.round(r.width), h: Math.round(r.height) }; };
  const iconSized = (r) => r.w > 0 && r.h > 0 && r.w <= 64 && r.h <= 64;
  const sel = (el) => {
    let s = el.tagName.toLowerCase();
    if (el.id) s += '#' + el.id;
    else if (typeof el.className === 'string' && el.className.trim()) s += '.' + el.className.trim().split(/\s+/).slice(0, 2).join('.');
    return s.slice(0, 120);
  };
  const ex = (arr, el, extra) => { if (arr.length < 3) arr.push(Object.assign({ sel: sel(el), rect: rectOf(el) }, extra || {})); };

  // ---------- ICONS ----------
  const icons = {
    inline_svg: { count: 0, icon_sized: 0, examples: [] },
    svg_use_local: { count: 0, examples: [] },
    svg_use_external: { count: 0, examples: [] },
    img_svg: { count: 0, examples: [] },
    css_bg_svg: { count: 0, examples: [] },
    mask_image_url: { count: 0, svg: 0, examples: [] },
    mask_image_gradient: { count: 0, examples: [] },
    icon_font_pua: { count: 0, families: {}, examples: [] },
  };
  for (const svg of document.querySelectorAll('svg')) {
    if (svg.ownerSVGElement) continue; // only outermost
    icons.inline_svg.count++;
    const r = rectOf(svg);
    if (iconSized(r)) icons.inline_svg.icon_sized++;
    ex(icons.inline_svg.examples, svg);
  }
  for (const use of document.querySelectorAll('use')) {
    const href = use.getAttribute('href') || use.getAttribute('xlink:href') || use.getAttributeNS('http://www.w3.org/1999/xlink', 'href') || '';
    const bucket = href.startsWith('#') ? icons.svg_use_local : icons.svg_use_external;
    bucket.count++;
    ex(bucket.examples, use, { href: href.slice(0, 120) });
  }
  for (const img of document.querySelectorAll('img')) {
    const src = img.currentSrc || img.src || '';
    if (/^data:image\/svg\+xml/i.test(src) || /\.svg(\?|#|$)/i.test(src)) {
      icons.img_svg.count++;
      ex(icons.img_svg.examples, img, { src: src.slice(0, 120) });
    }
  }
  const fontFamilyOf = (cs) => (cs.fontFamily || '').split(',')[0].trim().replace(/["']/g, '');
  for (const el of all) {
    const cs = getComputedStyle(el);
    if (isSvgUrl(cs.backgroundImage)) { icons.css_bg_svg.count++; ex(icons.css_bg_svg.examples, el); }
    const m = cs.maskImage && cs.maskImage !== 'none' ? cs.maskImage : (cs.webkitMaskImage && cs.webkitMaskImage !== 'none' ? cs.webkitMaskImage : '');
    if (m) {
      if (hasUrl(m)) { icons.mask_image_url.count++; if (isSvgUrl(m)) icons.mask_image_url.svg++; ex(icons.mask_image_url.examples, el, { mask: m.slice(0, 100) }); }
      else if (hasGrad(m)) { icons.mask_image_gradient.count++; ex(icons.mask_image_gradient.examples, el); }
    }
    for (const pseudo of ['::before', '::after']) {
      const pc = getComputedStyle(el, pseudo).content;
      if (pc && pc !== 'none' && pc !== 'normal' && PUA.test(pc)) {
        icons.icon_font_pua.count++;
        const fam = fontFamilyOf(getComputedStyle(el, pseudo));
        icons.icon_font_pua.families[fam] = (icons.icon_font_pua.families[fam] || 0) + 1;
        ex(icons.icon_font_pua.examples, el, { via: pseudo, family: fam });
      }
    }
  }
  const tw = document.createTreeWalker(document.body || document.documentElement, NodeFilter.SHOW_TEXT);
  let tn, tcount = 0;
  while ((tn = tw.nextNode()) && tcount++ < 100000) {
    if (PUA.test(tn.nodeValue) && tn.parentElement) {
      const p = tn.parentElement;
      if (p.closest('script,style,noscript')) continue;
      icons.icon_font_pua.count++;
      const fam = fontFamilyOf(getComputedStyle(p));
      icons.icon_font_pua.families[fam] = (icons.icon_font_pua.families[fam] || 0) + 1;
      ex(icons.icon_font_pua.examples, p, { via: 'text', family: fam });
    }
  }

  // ---------- LAZY LOAD ----------
  const imgs = Array.from(document.querySelectorAll('img'));
  const lazy = {
    img_total: imgs.length,
    native_loading_lazy_img: imgs.filter(i => (i.getAttribute('loading') || '').toLowerCase() === 'lazy').length,
    native_loading_lazy_iframe: Array.from(document.querySelectorAll('iframe')).filter(i => (i.getAttribute('loading') || '').toLowerCase() === 'lazy').length,
    data_src_attr: all.filter(e => e.hasAttribute('data-src') || e.hasAttribute('data-srcset') || e.hasAttribute('data-lazy') || e.hasAttribute('data-lazy-src') || e.hasAttribute('data-original') || e.hasAttribute('data-bg')).length,
    lazy_class: all.filter(e => typeof e.className === 'string' && /(^|[\s_-])lazy/i.test(e.className)).length,
    placeholder_src: imgs.filter(i => /^data:image\/(gif|png|svg)/i.test(i.getAttribute('src') || '') && (i.hasAttribute('data-src') || i.hasAttribute('data-srcset') || (i.naturalWidth <= 1 && i.naturalHeight <= 1))).length,
    srcset_img: imgs.filter(i => i.hasAttribute('srcset')).length,
    picture_el: document.querySelectorAll('picture').length,
    decoding_async: imgs.filter(i => (i.getAttribute('decoding') || '').toLowerCase() === 'async').length,
    fetchpriority_high: all.filter(e => (e.getAttribute('fetchpriority') || '').toLowerCase() === 'high').length,
    content_visibility_auto: all.filter(e => getComputedStyle(e).contentVisibility === 'auto').length,
    noscript_img: Array.from(document.querySelectorAll('noscript')).filter(n => /<img/i.test(n.textContent || '')).length,
    intersection_observer: (window.__censusIO || { ctor: 0, observe: 0 }),
  };

  // ---------- STACKING CONTEXTS (header / nav / menus) ----------
  const REGION_RE = /(^|[\s_-])(header|masthead|topbar|top-bar|navbar|nav|navigation|menu|menubar|dropdown|flyout|megamenu|mega-menu|banner)([\s_-]|$)/i;
  const roots = new Set();
  for (const e of document.querySelectorAll('header,nav,[role=banner],[role=navigation],[role=menu],[role=menubar],[aria-haspopup]')) roots.add(e);
  for (const e of all) {
    const id = e.id || ''; const cl = typeof e.className === 'string' ? e.className : '';
    if (REGION_RE.test(id) || REGION_RE.test(cl)) roots.add(e);
  }
  const region = new Set();
  for (const r of roots) { region.add(r); for (const d of r.querySelectorAll('*')) { if (region.size > 8000) break; region.add(d); } }
  const reasonsOf = (el) => {
    const cs = getComputedStyle(el); const out = [];
    const z = cs.zIndex; const pos = cs.position;
    const pcs = el.parentElement ? getComputedStyle(el.parentElement) : null;
    const flexGridItem = pcs && /flex|grid/.test(pcs.display);
    if (pos === 'fixed') out.push('position:fixed');
    if (pos === 'sticky') out.push('position:sticky');
    if ((pos === 'absolute' || pos === 'relative') && z !== 'auto') out.push('positioned+z-index');
    if (flexGridItem && pos === 'static' && z !== 'auto') out.push('flex/grid-item+z-index');
    if (parseFloat(cs.opacity) < 1) out.push('opacity<1');
    if (cs.transform !== 'none') out.push('transform');
    if ((cs.translate && cs.translate !== 'none') || (cs.rotate && cs.rotate !== 'none') || (cs.scale && cs.scale !== 'none')) out.push('translate/rotate/scale');
    if (cs.filter !== 'none') out.push('filter');
    if (cs.backdropFilter && cs.backdropFilter !== 'none' || cs.webkitBackdropFilter && cs.webkitBackdropFilter !== 'none') out.push('backdrop-filter');
    if (cs.mixBlendMode !== 'normal') out.push('mix-blend-mode');
    if (cs.isolation === 'isolate') out.push('isolation:isolate');
    if (cs.perspective !== 'none') out.push('perspective');
    if (cs.clipPath !== 'none') out.push('clip-path');
    if ((cs.maskImage && cs.maskImage !== 'none') || (cs.webkitMaskImage && cs.webkitMaskImage !== 'none')) out.push('mask');
    if (/opacity|transform|translate|rotate|scale|filter|perspective|clip-path|mask|isolation|mix-blend-mode|z-index/.test(cs.willChange)) out.push('will-change');
    if (/layout|paint|strict|content/.test(cs.contain)) out.push('contain');
    if (cs.containerType === 'size' || cs.containerType === 'inline-size') out.push('container-type');
    return { out, z };
  };
  const stacking = { region_roots: roots.size, region_elements: region.size, creators: 0, by_reason: {}, z_index_max: null, z_index_values: {}, examples: [] };
  for (const el of region) {
    const { out, z } = reasonsOf(el);
    if (!out.length) continue;
    stacking.creators++;
    for (const r of out) stacking.by_reason[r] = (stacking.by_reason[r] || 0) + 1;
    if (z !== 'auto') { const zi = parseInt(z, 10); if (!isNaN(zi)) { stacking.z_index_max = stacking.z_index_max === null ? zi : Math.max(stacking.z_index_max, zi); stacking.z_index_values[zi] = (stacking.z_index_values[zi] || 0) + 1; } }
    if (stacking.examples.length < 5) stacking.examples.push({ sel: sel(el), reasons: out, z, rect: rectOf(el) });
  }

  // ---------- OBJECT-FIT ----------
  const objectFit = { by_value: {}, non_default_position: 0, examples: [] };
  for (const el of document.querySelectorAll('img,video,picture img')) {
    const cs = getComputedStyle(el);
    if (cs.objectFit && cs.objectFit !== 'fill') {
      objectFit.by_value[cs.objectFit] = (objectFit.by_value[cs.objectFit] || 0) + 1;
      ex(objectFit.examples, el, { fit: cs.objectFit });
    }
    if (cs.objectPosition && cs.objectPosition !== '50% 50%') objectFit.non_default_position++;
  }

  return { url: location.href, title: document.title.slice(0, 120), elements_scanned: all.length, icons, lazy, stacking, objectFit };
}
