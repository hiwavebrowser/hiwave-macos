//! Autofill hardening: credentials are delivered only for a user-requested
//! fill on the page's own origin.
//!
//! Platform-neutral, so every port makes the same decision; the
//! platform glue in `main.rs` only reads the content view's URL, calls
//! these functions and evaluates what they return.
//!
//! 1. An app-side user action (`TriggerAutofill` from the chrome, a menu
//!    or an app shortcut, never page script) calls [`arm_autofill`] with
//!    the content view's key and its current top-level URL as the app
//!    reads it from the view.
//! 2. The page helper's request is answered by
//!    [`answer_content_autofill_request`]: once per arming, within
//!    [`AUTOFILL_ARM_TTL`], for the same view and origin, and only with
//!    usernames and opaque ids.
//! 3. The user's pick becomes a fill through [`autofill_fill_script`]:
//!    once, for an id from the current offer, for the same view and
//!    origin. The script re-checks the origin in the page before it fills.

use crate::state::AppState;
use serde::Serialize;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::time::{Duration, Instant};
use url::Url;

/// How long an arming (and the offer it produced) stays valid.
pub const AUTOFILL_ARM_TTL: Duration = Duration::from_secs(20);

/// One choice in the picker: a username and an opaque id for this offer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AutofillEntry {
    pub id: String,
    pub username: String,
}

/// The origin (`scheme://host[:port]`) of a top-level URL, for http(s)
/// pages with a host; `None` for anything else.
pub fn page_origin(top_level_url: &str) -> Option<String> {
    let url = Url::parse(top_level_url).ok()?;
    if !matches!(url.scheme(), "https" | "http") || url.host_str().is_none() {
        return None;
    }
    Some(url.origin().ascii_serialization())
}

#[derive(Debug)]
struct Armed {
    view: String,
    origin: String,
    at: Instant,
}

#[derive(Debug)]
struct Offer {
    view: String,
    origin: String,
    at: Instant,
    /// (opaque id, vault row id)
    entries: Vec<(String, i64)>,
}

/// Runtime autofill state: at most one arming and one open offer.
#[derive(Debug, Default)]
pub struct AutofillGate {
    armed: Option<Armed>,
    offer: Option<Offer>,
    counter: u64,
}

fn fresh(at: Instant, now: Instant) -> bool {
    now.checked_duration_since(at)
        .map_or(false, |age| age <= AUTOFILL_ARM_TTL)
}

impl AutofillGate {
    /// Arm one request for `view` at the origin of `top_level_url`.
    /// Replaces any earlier arming and closes any open offer.
    pub fn arm(&mut self, view: &str, top_level_url: &str, now: Instant) -> bool {
        self.offer = None;
        self.armed = page_origin(top_level_url).map(|origin| Armed {
            view: view.to_string(),
            origin,
            at: now,
        });
        self.armed.is_some()
    }

    /// Consume the arming. `Some(origin)` only when it is fresh, for this
    /// view, and the view is still at the armed origin; a named domain, if
    /// any, must be that origin's host.
    pub fn take_request(
        &mut self,
        view: &str,
        top_level_url: &str,
        requested_domain: Option<&str>,
        now: Instant,
    ) -> Option<String> {
        let armed = self.armed.take()?;
        let origin = page_origin(top_level_url)?;
        if armed.view != view || armed.origin != origin || !fresh(armed.at, now) {
            return None;
        }
        if let Some(domain) = requested_domain.filter(|d| !d.is_empty()) {
            let host = Url::parse(&origin).ok()?.host_str()?.to_ascii_lowercase();
            if !domain.eq_ignore_ascii_case(&host) && !domain.eq_ignore_ascii_case(&origin) {
                return None;
            }
        }
        Some(origin)
    }

    /// Open an offer for `rows` (vault id, username) and give the picker
    /// its entries.
    pub fn offer(
        &mut self,
        view: &str,
        origin: &str,
        rows: impl IntoIterator<Item = (i64, String)>,
        now: Instant,
    ) -> Vec<AutofillEntry> {
        let mut entries = Vec::new();
        let mut picker = Vec::new();
        for (row, username) in rows {
            self.counter += 1;
            let mut h = RandomState::new().build_hasher();
            h.write_u64(self.counter);
            let id = format!("{:016x}", h.finish());
            entries.push((id.clone(), row));
            picker.push(AutofillEntry { id, username });
        }
        self.offer = (!entries.is_empty()).then(|| Offer {
            view: view.to_string(),
            origin: origin.to_string(),
            at: now,
            entries,
        });
        picker
    }

    /// Consume the offer for a pick. `Some((origin, vault row id))` only
    /// for an id in a fresh offer for this view while the view is still at
    /// the offer's origin.
    pub fn take_pick(
        &mut self,
        view: &str,
        top_level_url: &str,
        entry_id: &str,
        now: Instant,
    ) -> Option<(String, i64)> {
        let offer = self.offer.take()?;
        let origin = page_origin(top_level_url)?;
        if offer.view != view || offer.origin != origin || !fresh(offer.at, now) {
            return None;
        }
        let row = offer.entries.iter().find(|(id, _)| id == entry_id)?.1;
        Some((origin, row))
    }
}

/// Arm one autofill request for `view` (an app-side user action only).
pub fn arm_autofill(state: &mut AppState, view: &str, top_level_url: &str, now: Instant) -> bool {
    state.autofill.arm(view, top_level_url, now)
}

/// Answer a content page's autofill request. `view` and `top_level_url`
/// come from the app's view of the content, never from the request;
/// `requested_domain` is what the request named, if anything. Gives
/// usernames and opaque ids only, and nothing unless an arming matches.
pub fn answer_content_autofill_request(
    state: &mut AppState,
    view: &str,
    top_level_url: &str,
    requested_domain: Option<&str>,
    now: Instant,
) -> Vec<AutofillEntry> {
    let Some(origin) = state.autofill.take_request(view, top_level_url, requested_domain, now) else {
        return Vec::new();
    };
    let rows: Vec<(i64, String)> = vault_rows(state, &origin)
        .into_iter()
        .map(|c| (c.id, c.username))
        .collect();
    state.autofill.offer(view, &origin, rows, now)
}

/// The fill for a pick from the current offer, as a script for the
/// content view; `None` unless the offer, view and origin all match.
pub fn autofill_fill_script(
    state: &mut AppState,
    view: &str,
    top_level_url: &str,
    entry_id: &str,
    now: Instant,
) -> Option<String> {
    let (origin, row) = state.autofill.take_pick(view, top_level_url, entry_id, now)?;
    let cred = vault_rows(state, &origin).into_iter().find(|c| c.id == row)?;
    Some(fill_script(&origin, &cred.username, &cred.password))
}

/// The script that shows the picker in the page helper.
pub fn picker_script(entries: &[AutofillEntry]) -> String {
    let list = serde_json::to_string(entries).unwrap_or_else(|_| "[]".to_string());
    format!(
        "(function(l){{var a=window.hiwaveAutofill;if(a&&typeof a.showCredentials==='function'){{a.showCredentials(l);}}}})({list});"
    )
}

fn vault_rows(state: &AppState, origin: &str) -> Vec<hiwave_vault::Credential> {
    if !state.vault.is_unlocked() {
        return Vec::new();
    }
    let Ok(url) = Url::parse(origin) else { return Vec::new() };
    state.vault.get_credentials(&url).unwrap_or_default()
}

fn js_literal(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string())
}

/// A self-contained fill: it runs only while the page is still at
/// `origin`, and does not call into any page-reachable global.
fn fill_script(origin: &str, username: &str, password: &str) -> String {
    let (o, u, p) = (js_literal(origin), js_literal(username), js_literal(password));
    format!(
        r#"(function(o,u,p){{
if(window.location.origin!==o)return;
var pw=document.querySelector('input[type="password"]:not([disabled])');
if(!pw)return;
var setter=Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set;
function put(el,v){{setter.call(el,v);el.dispatchEvent(new Event('input',{{bubbles:true}}));el.dispatchEvent(new Event('change',{{bubbles:true}}));}}
var scope=pw.closest('form')||document;
var sels=['input[type="email"]:not([disabled])','input[type="text"][name*="user"]:not([disabled])','input[type="text"][name*="email"]:not([disabled])','input[type="text"][name*="login"]:not([disabled])','input[type="text"][autocomplete="username"]:not([disabled])','input[type="text"][autocomplete="email"]:not([disabled])','input[type="text"]:not([disabled])'];
var user=null;
for(var i=0;i<sels.length;i++){{user=scope.querySelector(sels[i]);if(user&&user!==pw)break;user=null;}}
if(user)put(user,u);
put(pw,p);
}})({o},{u},{p});"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::{commands, IpcMessage, IpcResponse};
    use crate::state::{AppConfig, AppState};
    use std::sync::{Arc, Mutex};
    use url::Url;

    const SECRET: &str = "s3cret-pw-for-tests";
    const OTHER_SECRET: &str = "other-secret";
    const PAGE: &str = "https://a.example/login?next=1";
    const VIEW: &str = "tab-1";

    fn state_with_vault(dir: &std::path::Path) -> AppState {
        let config = AppConfig {
            data_dir: dir.to_path_buf(),
            download_dir: dir.join("downloads"),
            ..Default::default()
        };
        let mut state = AppState::new(config).expect("app state in a temp dir");
        state.vault.unlock("master").expect("unlock");
        state
            .vault
            .save_credential(&Url::parse("https://a.example/login").unwrap(), "alice", SECRET)
            .expect("save");
        state
            .vault
            .save_credential(&Url::parse("https://b.example/login").unwrap(), "bob", OTHER_SECRET)
            .expect("save");
        state
    }

    /// A request that reaches the shared command handler (the path a page's
    /// own message took) is never answered with credential material.
    #[test]
    fn an_unsolicited_request_gets_no_credentials() {
        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(Mutex::new(state_with_vault(dir.path())));
        let response = commands::handle_message(
            &state,
            IpcMessage::GetCredentialsForAutofill { domain: "a.example".into() },
        );
        let json = serde_json::to_string(&response).unwrap();
        assert!(!json.contains(SECRET), "response carried a password: {json}");
        assert!(!json.contains("alice"), "response carried a username: {json}");
        assert!(matches!(response, IpcResponse::Error { .. }), "{json}");
    }

    /// Without an app-side arming the content request gets nothing.
    #[test]
    fn an_unarmed_content_request_gets_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = state_with_vault(dir.path());
        let now = Instant::now();
        assert!(answer_content_autofill_request(&mut state, VIEW, PAGE, None, now).is_empty());
        assert!(answer_content_autofill_request(&mut state, VIEW, PAGE, Some("a.example"), now).is_empty());
    }

    /// An armed request gets usernames and opaque ids for the page's own
    /// origin only, once.
    #[test]
    fn an_armed_request_gets_usernames_and_ids_only() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = state_with_vault(dir.path());
        let now = Instant::now();
        assert!(arm_autofill(&mut state, VIEW, PAGE, now));
        let entries = answer_content_autofill_request(&mut state, VIEW, PAGE, None, now);
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].username, "alice");
        assert_ne!(entries[0].id, "1", "the id is opaque, not the vault row id");
        let picker = picker_script(&entries);
        assert!(!picker.contains(SECRET) && !picker.contains(OTHER_SECRET), "{picker}");
        assert!(!picker.contains("bob"), "{picker}");
        // One-shot.
        assert!(answer_content_autofill_request(&mut state, VIEW, PAGE, None, now).is_empty());
    }

    /// A request naming a domain other than the page's own gets nothing,
    /// and so does a page that is not at the armed origin.
    #[test]
    fn a_request_for_another_domain_gets_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = state_with_vault(dir.path());
        let now = Instant::now();
        arm_autofill(&mut state, VIEW, PAGE, now);
        assert!(answer_content_autofill_request(&mut state, VIEW, PAGE, Some("b.example"), now).is_empty());

        arm_autofill(&mut state, VIEW, PAGE, now);
        assert!(answer_content_autofill_request(&mut state, VIEW, "https://b.example/", None, now).is_empty());

        arm_autofill(&mut state, VIEW, PAGE, now);
        assert!(answer_content_autofill_request(&mut state, "tab-2", PAGE, None, now).is_empty());

        // The page's own host, named, is fine.
        arm_autofill(&mut state, VIEW, PAGE, now);
        assert_eq!(answer_content_autofill_request(&mut state, VIEW, PAGE, Some("a.example"), now).len(), 1);
    }

    #[test]
    fn an_arming_expires() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = state_with_vault(dir.path());
        let now = Instant::now();
        arm_autofill(&mut state, VIEW, PAGE, now);
        let later = now + AUTOFILL_ARM_TTL + Duration::from_secs(1);
        assert!(answer_content_autofill_request(&mut state, VIEW, PAGE, None, later).is_empty());
    }

    #[test]
    fn a_pick_fills_once_on_the_same_origin() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = state_with_vault(dir.path());
        let now = Instant::now();
        arm_autofill(&mut state, VIEW, PAGE, now);
        let entries = answer_content_autofill_request(&mut state, VIEW, PAGE, None, now);
        let script = autofill_fill_script(&mut state, VIEW, PAGE, &entries[0].id, now).expect("fill");
        assert!(script.contains(&js_literal(SECRET)));
        assert!(script.contains("\"https://a.example\""), "the script re-checks the origin");
        assert!(!script.contains(OTHER_SECRET));
        assert!(autofill_fill_script(&mut state, VIEW, PAGE, &entries[0].id, now).is_none(), "one-shot");
    }

    /// After the view navigates to another origin, a pick fills nothing.
    #[test]
    fn a_fill_after_the_origin_changed_does_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = state_with_vault(dir.path());
        let now = Instant::now();
        arm_autofill(&mut state, VIEW, PAGE, now);
        let entries = answer_content_autofill_request(&mut state, VIEW, PAGE, None, now);
        assert!(autofill_fill_script(&mut state, VIEW, "https://b.example/login", &entries[0].id, now).is_none());

        // Nor for an id not in the offer, nor in another view.
        arm_autofill(&mut state, VIEW, PAGE, now);
        let entries = answer_content_autofill_request(&mut state, VIEW, PAGE, None, now);
        assert!(autofill_fill_script(&mut state, VIEW, PAGE, "1", now).is_none());
        arm_autofill(&mut state, VIEW, PAGE, now);
        let _ = answer_content_autofill_request(&mut state, VIEW, PAGE, None, now);
        assert!(autofill_fill_script(&mut state, "tab-2", PAGE, &entries[0].id, now).is_none());
    }

    /// rustkit-engine's `autofill_scripts_mid_load_leave_the_form_and_the_load_alone`
    /// evaluates this script verbatim.
    #[test]
    fn the_picker_script_is_the_one_the_engine_test_runs() {
        let entries = [AutofillEntry { id: "0123456789abcdef".into(), username: "u".into() }];
        assert_eq!(
            picker_script(&entries),
            "(function(l){var a=window.hiwaveAutofill;if(a&&typeof a.showCredentials==='function'){a.showCredentials(l);}})([{\"id\":\"0123456789abcdef\",\"username\":\"u\"}]);"
        );
    }

    #[test]
    fn literals_are_escaped() {
        let s = fill_script("https://a.example", "a\"b", "x'\\</script>\u{2028}");
        assert!(s.contains(r#""a\"b""#), "{s}");
        assert!(s.contains(r#""x'\\</script>"#), "{s}");
    }

    #[test]
    fn only_http_pages_have_an_origin() {
        assert_eq!(page_origin("https://a.example:8443/x").as_deref(), Some("https://a.example:8443"));
        assert_eq!(page_origin("about:blank"), None);
        assert_eq!(page_origin("data:text/html,x"), None);
        assert_eq!(page_origin("file:///etc/hosts"), None);
    }

    /// A row saved for an https host is not offered to that host over http.
    #[test]
    fn an_http_page_gets_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = state_with_vault(dir.path());
        let now = Instant::now();
        let page = "http://a.example/login";
        arm_autofill(&mut state, VIEW, page, now);
        assert!(answer_content_autofill_request(&mut state, VIEW, page, None, now).is_empty());
    }

    /// Nor on another port of the same host.
    #[test]
    fn another_port_gets_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = state_with_vault(dir.path());
        let now = Instant::now();
        let page = "https://a.example:8443/login";
        arm_autofill(&mut state, VIEW, page, now);
        assert!(answer_content_autofill_request(&mut state, VIEW, page, None, now).is_empty());
    }

    /// The same https origin, default port spelled out or not, is offered.
    #[test]
    fn the_same_https_origin_is_offered() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = state_with_vault(dir.path());
        let now = Instant::now();
        for page in ["https://a.example/", "https://a.example:443/login", "https://A.EXAMPLE/x"] {
            arm_autofill(&mut state, VIEW, page, now);
            let entries = answer_content_autofill_request(&mut state, VIEW, page, None, now);
            assert_eq!(entries.len(), 1, "{page}: {entries:?}");
            assert!(autofill_fill_script(&mut state, VIEW, page, &entries[0].id, now).is_some(), "{page}");
        }
    }

    #[test]
    fn an_offer_expires() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = state_with_vault(dir.path());
        let now = Instant::now();
        arm_autofill(&mut state, VIEW, PAGE, now);
        let entries = answer_content_autofill_request(&mut state, VIEW, PAGE, None, now);
        let later = now + AUTOFILL_ARM_TTL + Duration::from_secs(1);
        assert!(autofill_fill_script(&mut state, VIEW, PAGE, &entries[0].id, later).is_none());
    }

    /// An id from an earlier offer does nothing after a re-arm on the same
    /// view and origin.
    #[test]
    fn an_old_id_after_a_rearm_does_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = state_with_vault(dir.path());
        let now = Instant::now();
        arm_autofill(&mut state, VIEW, PAGE, now);
        let first = answer_content_autofill_request(&mut state, VIEW, PAGE, None, now);
        arm_autofill(&mut state, VIEW, PAGE, now);
        let second = answer_content_autofill_request(&mut state, VIEW, PAGE, None, now);
        assert_ne!(first[0].id, second[0].id);
        assert!(autofill_fill_script(&mut state, VIEW, PAGE, &first[0].id, now).is_none());
        // ...and the mismatched pick cleared the offer (fail-safe).
        assert!(autofill_fill_script(&mut state, VIEW, PAGE, &second[0].id, now).is_none());
    }

    const HELPER: &str = include_str!("../ui/autofill.js");

    /// The helper takes its list only from the app: no window message
    /// listener, and it does not stand down for a pre-existing global.
    #[test]
    fn the_page_helper_takes_its_list_only_from_the_app() {
        assert!(!HELPER.contains("addEventListener('message'"), "helper listens for window messages");
        assert!(!HELPER.contains("addEventListener(\"message\""), "helper listens for window messages");
        assert!(!HELPER.contains("if (window.hiwaveAutofill) return"), "helper skips setup for an existing global");
        assert!(!HELPER.contains(".password"), "helper handles credential secrets");
    }
}
