// never_load.mjs - hosts no census crawler may reach, shared by every crawler in this directory.
// Two checks: the resolver refuses these names at Chromium launch, so a redirect cannot reach them
// either (a request handler sees only the first URL of a redirect chain); and every request a context
// makes is watched, so a site whose load touched one of them can be dropped.

// Each label refuses that domain under any suffix and all its subdomains: `ebay` covers ebay.com,
// ebay.co.uk and www.ebay.de.
const LABELS = ['ebay', 'ebayimg', 'ebaystatic', 'ebaydesc', 'ebayinc',
  'shopify', 'myshopify', 'shopifycdn', 'shopifycloud', 'shopifysvc'];
// Exact domains, with their subdomains.
const DOMAINS = ['shop.app'];

const escape = (s) => s.replace(/\./g, '\\.');
const REFUSED = new RegExp(
  `(^|\\.)((${LABELS.join('|')})\\.|(${DOMAINS.map(escape).join('|')})\\.?$)`,
  'i',
);

export function neverLoad(url) {
  try {
    return REFUSED.test(new URL(url).hostname);
  } catch {
    return true;
  }
}

// Chromium's --host-resolver-rules: the first matching rule wins, so these go before any other rule.
export function hostResolverRules() {
  return [
    ...LABELS.flatMap((l) => [`MAP ${l}.* ~NOTFOUND`, `MAP *.${l}.* ~NOTFOUND`]),
    ...DOMAINS.flatMap((d) => [`MAP ${d} ~NOTFOUND`, `MAP *.${d} ~NOTFOUND`]),
  ].join(', ');
}

// `extraRules` come after the refusals (a test maps everything else to a local server).
export async function launchRefusing(chromium, opts = {}, extraRules = []) {
  const rules = [hostResolverRules(), ...extraRules].join(', ');
  return chromium.launch({ ...opts, args: [...(opts.args || []), `--host-resolver-rules=${rules}`] });
}

// Abort refused requests the handler can see, and record every request event, which redirect hops
// do raise. `touched` true means the site's load reached for a refused host: drop it.
export async function watchRefused(ctx) {
  const state = { touched: false, refused: 0 };
  ctx.on('request', (req) => {
    if (neverLoad(req.url())) state.touched = true;
  });
  await ctx.route('**/*', (route) => {
    if (!neverLoad(route.request().url())) return route.continue();
    state.touched = true;
    state.refused += 1;
    return route.abort();
  });
  return state;
}
