//! The cookie jar (RFC 6265 §5): what `Set-Cookie` and `document.cookie`
//! store, and the `Cookie` header each request carries.
//!
//! In memory, shared by every client cloned from the one that made it, and
//! gone with the process. A `Domain` attribute naming a public suffix
//! (`co.uk`, `github.io`) is refused, so one site cannot set cookies for its
//! siblings. Not here (stated, not half-built): `SameSite` enforcement and
//! cookie prefixes.

use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, SystemTime};

use url::Url;

/// Most cookies the jar keeps; past it the least recently set go first.
const MAX_COOKIES: usize = 3000;
/// Largest name plus value kept (RFC 6265 §6.1 asks for at least 4096).
const MAX_COOKIE_BYTES: usize = 4096;

#[derive(Debug, Clone)]
struct Cookie {
    name: String,
    value: String,
    /// Lowercase, without a leading dot.
    domain: String,
    host_only: bool,
    path: String,
    expires: Option<SystemTime>,
    secure: bool,
    http_only: bool,
    /// Set order, for the Cookie header's tie-break and eviction.
    created: u64,
}

/// Who is writing a cookie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CookieSource {
    /// A response's `Set-Cookie`.
    Http,
    /// `document.cookie = ...`: may not set or replace an `HttpOnly` one.
    Script,
}

#[derive(Debug, Default)]
pub struct CookieJar {
    inner: Mutex<JarState>,
}

#[derive(Debug, Default)]
struct JarState {
    cookies: Vec<Cookie>,
    counter: u64,
    subscriptions: Vec<Watcher>,
}

/// A mutation visible to a particular document. Deletions carry only the name.
#[derive(Debug, PartialEq, Eq)]
pub enum CookieChange {
    Changed { name: String, value: String },
    Deleted { name: String },
}

#[derive(Debug, Default)]
pub struct CookieChanges {
    pub changes: Vec<CookieChange>,
    /// The receiver fell behind. Its consumer should re-read the jar.
    pub overflowed: bool,
}

/// A document's subscription. Dropping it disconnects it from the jar.
#[derive(Debug)]
pub struct CookieSubscription(Arc<Mutex<CookieChanges>>);

impl CookieSubscription {
    pub fn drain(&self) -> CookieChanges {
        std::mem::take(&mut *self.0.lock().unwrap())
    }
}

#[derive(Debug)]
struct Watcher {
    url: Url,
    queue: Weak<Mutex<CookieChanges>>,
}

impl JarState {
    fn notify(&mut self, before: Option<&Cookie>, after: Option<&Cookie>) {
        const MAX_PENDING_CHANGES: usize = 256;
        self.subscriptions.retain(|watcher| {
            let Some(queue) = watcher.queue.upgrade() else { return false };
            let visible = |cookie: &&Cookie| cookie.visible_at(&watcher.url, true);
            let change = match (before.filter(visible), after.filter(visible)) {
                (Some(old), Some(new)) if old.value == new.value => return true,
                (_, Some(new)) => CookieChange::Changed { name: new.name.clone(), value: new.value.clone() },
                (Some(old), None) => CookieChange::Deleted { name: old.name.clone() },
                (None, None) => return true,
            };
            let mut pending = queue.lock().unwrap();
            if !pending.overflowed {
                if pending.changes.len() == MAX_PENDING_CHANGES {
                    pending.changes.clear();
                    pending.overflowed = true;
                } else {
                    pending.changes.push(change);
                }
            }
            true
        });
    }
}

impl Cookie {
    fn visible_at(&self, url: &Url, for_script: bool) -> bool {
        (!for_script || !self.http_only)
            && url.host_str().is_some_and(|host| {
                if self.host_only { host == self.domain } else { domain_matches(host, &self.domain) }
            })
            && path_matches(url.path(), &self.path)
            && (!self.secure || is_secure(url))
    }
}

impl CookieJar {
    pub fn new() -> Self {
        Self::default()
    }

    /// Subscribe to future script-visible changes at a document's creation URL.
    pub fn subscribe(&self, url: &Url) -> Option<CookieSubscription> {
        if url.host_str().is_none() || !matches!(url.scheme(), "http" | "https") {
            return None;
        }
        let queue = Arc::new(Mutex::new(CookieChanges::default()));
        let mut state = self.inner.lock().unwrap();
        state.subscriptions.retain(|watcher| watcher.queue.strong_count() != 0);
        state.subscriptions.push(Watcher { url: url.clone(), queue: Arc::downgrade(&queue) });
        Some(CookieSubscription(queue))
    }

    /// Store one `Set-Cookie` value (or a `document.cookie` assignment)
    /// received for `url`. One that is malformed or not allowed for `url`
    /// changes nothing.
    pub fn set(&self, url: &Url, header: &str, source: CookieSource) {
        let Some(cookie) = parse(url, header, source) else { return };
        let now = SystemTime::now();
        let mut state = self.inner.lock().unwrap();
        let same = |c: &Cookie| c.name == cookie.name && c.domain == cookie.domain && c.path == cookie.path;
        let mut created = None;
        let old = state.cookies.iter().find(|c| same(c));
        if let Some(old) = old {
            // A replacement keeps the old one's place (§5.3 step 11).
            created = Some(old.created);
            if old.http_only && source == CookieSource::Script {
                return;
            }
            // A cookie set over plain http does not replace a secure one.
            if old.secure && url.scheme() != "https" {
                return;
            }
        }
        let before = old.filter(|c| !state.subscriptions.is_empty() && c.expires.is_none_or(|e| e > now)).cloned();
        state.cookies.retain(|c| !same(c) && c.expires.is_none_or(|e| e > now));
        if cookie.expires.is_some_and(|e| e <= now) {
            state.notify(before.as_ref(), None);
            return;
        }
        state.notify(before.as_ref(), Some(&cookie));
        state.counter += 1;
        let created = created.unwrap_or(state.counter);
        state.cookies.push(Cookie { created, ..cookie });
        if state.cookies.len() > MAX_COOKIES {
            state.cookies.sort_by_key(|c| c.created);
            let evicted = state.cookies.remove(0);
            state.notify(Some(&evicted), None);
        }
    }

    /// The `Cookie` header for a request to `url`, or `None` with nothing
    /// to send. `for_script` leaves out `HttpOnly` ones (`document.cookie`).
    pub fn header_for(&self, url: &Url, for_script: bool) -> Option<String> {
        url.host_str()?;
        let now = SystemTime::now();
        let guard = self.inner.lock().unwrap();
        let mut matching: Vec<&Cookie> = guard
            .cookies
            .iter()
            .filter(|c| c.expires.is_none_or(|e| e > now))
            .filter(|c| c.visible_at(url, for_script))
            .collect();
        if matching.is_empty() {
            return None;
        }
        // Longer paths first, then the earlier set (§5.4 step 2).
        matching.sort_by(|a, b| b.path.len().cmp(&a.path.len()).then(a.created.cmp(&b.created)));
        Some(
            matching
                .iter()
                .map(|c| if c.name.is_empty() { c.value.clone() } else { format!("{}={}", c.name, c.value) })
                .collect::<Vec<_>>()
                .join("; "),
        )
    }
}

/// https, or a loopback host (as browsers treat `http://localhost`).
fn is_secure(url: &Url) -> bool {
    url.scheme() == "https"
        || matches!(url.host(), Some(url::Host::Domain("localhost")))
        || matches!(url.host(), Some(url::Host::Ipv4(ip)) if ip.is_loopback())
        || matches!(url.host(), Some(url::Host::Ipv6(ip)) if ip.is_loopback())
}

/// On the public suffix list (or an unlisted top-level name, by its `*` rule).
fn is_public_suffix(domain: &str) -> bool {
    psl::suffix_str(domain) == Some(domain)
}

fn domain_matches(host: &str, domain: &str) -> bool {
    host == domain
        || (host.ends_with(domain)
            && host.as_bytes()[host.len() - domain.len() - 1] == b'.'
            && host.parse::<std::net::IpAddr>().is_err())
}

/// RFC 6265 §5.1.4.
fn path_matches(request: &str, cookie: &str) -> bool {
    request == cookie
        || (request.starts_with(cookie)
            && (cookie.ends_with('/') || request.as_bytes().get(cookie.len()) == Some(&b'/')))
}

/// The directory of the request path (§5.1.4 default-path).
fn default_path(url: &Url) -> String {
    let path = url.path();
    match path.rfind('/') {
        Some(0) | None => "/".into(),
        Some(i) => path[..i].into(),
    }
}

/// RFC 6265 §5.2, for `url`.
fn parse(url: &Url, header: &str, source: CookieSource) -> Option<Cookie> {
    let host = url.host_str()?.to_ascii_lowercase();
    let mut parts = header.split(';');
    let pair = parts.next()?;
    let (name, value) = match pair.split_once('=') {
        Some((n, v)) => (n.trim(), v.trim()),
        None => ("", pair.trim()),
    };
    if (name.is_empty() && value.is_empty()) || name.len() + value.len() > MAX_COOKIE_BYTES {
        return None;
    }
    if name.chars().chain(value.chars()).any(|c| c.is_control()) {
        return None;
    }
    let mut cookie = Cookie {
        name: name.into(),
        value: value.into(),
        domain: host.clone(),
        host_only: true,
        path: default_path(url),
        expires: None,
        secure: false,
        http_only: false,
        created: 0,
    };
    let mut max_age = None;
    for attr in parts {
        let (key, val) = match attr.split_once('=') {
            Some((k, v)) => (k.trim(), v.trim()),
            None => (attr.trim(), ""),
        };
        match key.to_ascii_lowercase().as_str() {
            "expires" => {
                if let Ok(when) = httpdate::parse_http_date(val) {
                    cookie.expires = Some(when);
                }
            }
            "max-age" => {
                if let Ok(secs) = val.parse::<i64>() {
                    max_age = Some(secs);
                }
            }
            "domain" if !val.is_empty() => {
                let domain = val.trim_start_matches('.').to_ascii_lowercase();
                if !domain_matches(&host, &domain) {
                    return None;
                }
                // A public suffix is no cookie's domain: refused, unless it
                // is the host itself, which keeps it host-only (§5.3 step 5).
                if is_public_suffix(&domain) {
                    if domain != host {
                        return None;
                    }
                    continue;
                }
                cookie.domain = domain;
                cookie.host_only = false;
            }
            "path" if val.starts_with('/') => cookie.path = val.into(),
            "secure" => cookie.secure = true,
            "httponly" => cookie.http_only = true,
            _ => {}
        }
    }
    // Max-Age wins over Expires (§5.3 step 3).
    if let Some(secs) = max_age {
        cookie.expires = Some(match secs {
            s if s <= 0 => SystemTime::UNIX_EPOCH,
            s => SystemTime::now() + Duration::from_secs(s as u64),
        });
    }
    if cookie.http_only && source == CookieSource::Script {
        return None;
    }
    if cookie.secure && !is_secure(url) {
        return None;
    }
    Some(cookie)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn subscriptions_report_scoped_script_visible_mutations() {
        let jar = CookieJar::new();
        let origin = url("https://a.example.com/shop/cart");
        let watch = jar.subscribe(&origin).unwrap();
        let off_path = jar.subscribe(&url("https://a.example.com/other")).unwrap();
        let sibling = jar.subscribe(&url("https://b.example.com/shop/cart")).unwrap();
        let insecure = jar.subscribe(&url("http://a.example.com/shop/cart")).unwrap();
        jar.set(&origin, "sid=first; Secure", CookieSource::Http);
        jar.set(&origin, "private=secret; HttpOnly", CookieSource::Http);
        jar.set(&origin, "sid=second; Secure", CookieSource::Script);
        jar.set(&origin, "sid=; Max-Age=0; Secure", CookieSource::Http);
        let batch = watch.drain();
        assert!(!batch.overflowed);
        assert_eq!(batch.changes, [
            CookieChange::Changed { name: "sid".into(), value: "first".into() },
            CookieChange::Changed { name: "sid".into(), value: "second".into() },
            CookieChange::Deleted { name: "sid".into() },
        ]);
        assert!(off_path.drain().changes.is_empty());
        assert!(sibling.drain().changes.is_empty());
        assert!(insecure.drain().changes.is_empty());
        assert!(watch.drain().changes.is_empty());
    }

    #[test]
    fn subscriptions_can_recover_after_overflow() {
        let jar = CookieJar::new();
        let origin = url("https://example.com/");
        let watch = jar.subscribe(&origin).unwrap();
        for value in 0..300 {
            jar.set(&origin, &format!("counter={value}"), CookieSource::Script);
        }
        let batch = watch.drain();
        assert!(batch.overflowed);
        assert!(batch.changes.is_empty());
        assert_eq!(jar.header_for(&origin, true).as_deref(), Some("counter=299"));
        jar.set(&origin, "counter=300", CookieSource::Script);
        let batch = watch.drain();
        assert!(!batch.overflowed);
        assert_eq!(batch.changes, [CookieChange::Changed { name: "counter".into(), value: "300".into() }]);
    }

    #[test]
    fn a_cookie_goes_back_to_its_host_and_path() {
        let jar = CookieJar::new();
        jar.set(&url("https://a.example.com/shop/cart"), "sid=1", CookieSource::Http);
        jar.set(&url("https://a.example.com/"), "lang=en; Path=/", CookieSource::Http);
        assert_eq!(jar.header_for(&url("https://a.example.com/shop/x"), false).as_deref(), Some("sid=1; lang=en"));
        assert_eq!(jar.header_for(&url("https://a.example.com/other"), false).as_deref(), Some("lang=en"));
        assert_eq!(jar.header_for(&url("https://b.example.com/shop/x"), false), None, "host-only");
        assert_eq!(jar.header_for(&url("https://a.example.com/shopping"), false).as_deref(), Some("lang=en"));
    }

    #[test]
    fn a_domain_cookie_reaches_subdomains_and_a_foreign_domain_is_refused() {
        let jar = CookieJar::new();
        jar.set(&url("https://www.example.com/"), "a=1; Domain=.example.com", CookieSource::Http);
        jar.set(&url("https://www.example.com/"), "b=2; Domain=other.com", CookieSource::Http);
        jar.set(&url("https://www.example.com/"), "c=3; Domain=com", CookieSource::Http);
        assert_eq!(jar.header_for(&url("https://api.example.com/"), false).as_deref(), Some("a=1"));
        assert_eq!(jar.header_for(&url("https://other.com/"), false), None);
    }

    #[test]
    fn a_public_suffix_is_no_cookies_domain() {
        let jar = CookieJar::new();
        jar.set(&url("https://a.co.uk/"), "sid=x; Domain=co.uk", CookieSource::Http);
        jar.set(&url("https://x.github.io/"), "sid=y; Domain=.github.io", CookieSource::Script);
        for site in ["https://bank.co.uk/", "https://a.co.uk/", "https://y.github.io/", "https://x.github.io/"] {
            assert_eq!(jar.header_for(&url(site), false), None, "{site}");
        }
        // The suffix's own host keeps it, host-only.
        jar.set(&url("https://github.io/"), "own=1; Domain=github.io", CookieSource::Http);
        assert_eq!(jar.header_for(&url("https://github.io/"), false).as_deref(), Some("own=1"));
        assert_eq!(jar.header_for(&url("https://x.github.io/"), false), None);
        // A registrable domain under a suffix still works.
        jar.set(&url("https://www.shop.co.uk/"), "ok=1; Domain=shop.co.uk", CookieSource::Http);
        assert_eq!(jar.header_for(&url("https://api.shop.co.uk/"), false).as_deref(), Some("ok=1"));
    }

    #[test]
    fn expiry_replacement_secure_and_http_only() {
        let jar = CookieJar::new();
        let u = url("https://example.com/");
        jar.set(&u, "a=1", CookieSource::Http);
        jar.set(&u, "a=2", CookieSource::Http);
        jar.set(&u, "gone=1; Max-Age=0", CookieSource::Http);
        jar.set(&u, "old=1; Expires=Thu, 01 Jan 1970 00:00:01 GMT", CookieSource::Http);
        jar.set(&u, "s=1; Secure", CookieSource::Http);
        jar.set(&u, "h=1; HttpOnly", CookieSource::Http);
        assert_eq!(jar.header_for(&u, false).as_deref(), Some("a=2; s=1; h=1"));
        assert_eq!(jar.header_for(&url("http://example.com/"), false).as_deref(), Some("a=2; h=1"));
        // Script sees and writes around HttpOnly, never over it.
        jar.set(&u, "h=evil", CookieSource::Script);
        jar.set(&u, "x=1; HttpOnly", CookieSource::Script);
        assert_eq!(jar.header_for(&u, true).as_deref(), Some("a=2; s=1"));
        assert_eq!(jar.header_for(&u, false).as_deref(), Some("a=2; s=1; h=1"));
        // Deleting by expiry.
        jar.set(&u, "a=; Max-Age=-1", CookieSource::Script);
        assert_eq!(jar.header_for(&u, true).as_deref(), Some("s=1"));
    }

    #[test]
    fn http_cannot_set_or_replace_a_secure_cookie_except_on_loopback() {
        let jar = CookieJar::new();
        jar.set(&url("http://example.com/"), "s=1; Secure", CookieSource::Http);
        assert_eq!(jar.header_for(&url("https://example.com/"), false), None);
        jar.set(&url("http://127.0.0.1:8000/"), "s=1; Secure", CookieSource::Http);
        assert_eq!(jar.header_for(&url("http://127.0.0.1:8000/"), false).as_deref(), Some("s=1"));
    }

    #[test]
    fn path_without_a_slash_control_chars_and_oversized_names_are_refused() {
        let jar = CookieJar::new();
        let u = url("https://example.com/shop/cart");
        // Path must start with `/` (§5.2.4); otherwise the default path stays.
        jar.set(&u, "a=1; Path=shop", CookieSource::Http);
        assert_eq!(
            jar.header_for(&url("https://example.com/shop/x"), false).as_deref(),
            Some("a=1")
        );
        assert_eq!(
            jar.header_for(&url("https://example.com/other"), false),
            None,
            "default path is /shop, not Path=shop"
        );
        // Control characters inside name or value are refused (§5.2).
        // A leading/trailing newline is trimmed before the check; mid-string is not.
        jar.set(&u, "ba\nd=1", CookieSource::Http);
        jar.set(&u, "ok=has\u{1}x", CookieSource::Http);
        assert_eq!(
            jar.header_for(&url("https://example.com/shop/x"), false).as_deref(),
            Some("a=1")
        );
        // Name+value over the 4096-byte floor is dropped.
        let huge = format!("n={}", "x".repeat(MAX_COOKIE_BYTES));
        jar.set(&u, &huge, CookieSource::Http);
        assert_eq!(
            jar.header_for(&url("https://example.com/shop/x"), false).as_deref(),
            Some("a=1")
        );
    }
}
