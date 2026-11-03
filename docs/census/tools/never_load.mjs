// never_load.mjs - hosts no census crawler may reach. Stub, red first.
export function neverLoad(url) {
  try {
    return /(^|\.)(ebay|ebayimg|ebaystatic|shopify|myshopify|shopifycdn)\./i.test(new URL(url).hostname);
  } catch {
    return true;
  }
}

export function hostResolverRules() {
  return '';
}

export async function launchRefusing(chromium, opts = {}, extraRules = []) {
  const rules = extraRules.join(', ');
  return chromium.launch({ ...opts, args: [...(opts.args || []), ...(rules ? [`--host-resolver-rules=${rules}`] : [])] });
}

export async function watchRefused(ctx) {
  return { touched: false, refused: 0 };
}
