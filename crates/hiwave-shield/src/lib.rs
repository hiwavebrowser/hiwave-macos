//! Ad blocking and privacy protection for HiWave
//!
//! Uses Brave's adblock-rust engine for high-performance ad and tracker blocking.

pub mod filter_lists;

use adblock::lists::ParseOptions;
use adblock::Engine;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use url::Url;
use hiwave_core::HiWaveResult;

pub use filter_lists::{FilterListManager, FilterListSource, FILTER_LISTS};

/// Which list, and which of its rules, blocked a request (dev-tools T2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockMatch {
    /// The list's name (`EasyList`, `EasyPrivacy`), `HiWave defaults` for
    /// the starter rules, `custom` for rules handed to `with_rules`, or
    /// [`UNATTRIBUTED`] when the engine cannot say which list it was.
    pub list: String,
    /// The rule as the list writes it; `None` when the engine was built
    /// without rule names. Rules the engine joined into one are written
    /// as the list writes each, separated by ` <+> `: one of them matched.
    pub rule: Option<String>,
}

/// The list named for a match that rules from more than one list could
/// have made: the engine joined rules from different lists into the one
/// that matched (` <+> `), or was built without rule names.
pub const UNATTRIBUTED: &str = "unattributed";

/// What adblock-rust writes between the rules it joined into one.
const JOINED: &str = " <+> ";

/// Ad blocker powered by Brave's adblock-rust engine
pub struct AdBlocker {
    engine: Engine,
    enabled: bool,
    requests_blocked: AtomicU64,
    trackers_blocked: AtomicU64,
    /// The list each rule came from, when rules came from more than one.
    rule_lists: HashMap<String, &'static str>,
    /// The list every rule came from, when rules came from one list.
    default_list: &'static str,
    /// Rules came from more than one list. Without rule names a match then
    /// cannot be traced to its list.
    several_lists: bool,
    /// The engine keeps each rule's text, so a match names its rule. About
    /// 9 MB more for the full lists, so `with_filter_lists` leaves it off
    /// (`with_filter_lists_naming_rules` turns it on).
    names_rules: bool,
}

impl AdBlocker {
    const DEFAULT_RULES: &'static [&'static str] = &[
        // Google advertising
        "||doubleclick.net^",
        "||googlesyndication.com^",
        "||googleadservices.com^",
        "||adtrafficquality.google^",
        "||google.com/recaptcha/api2/aframe^",
        // Social media ads/tracking
        "||ads.twitter.com^",
        "||facebook.com/tr^",
        "||connect.facebook.net^",
        "||tr.snapchat.com^",
        // Amazon
        "||amazon-adsystem.com^",
        // Criteo
        "||criteo.com/sync^",
        "||criteo.com/syncframe^",
        // Common ad networks
        "||adnxs.com^",
        "||adsrvr.org^",
        "||adroll.com^",
        "||taboola.com^",
        "||outbrain.com^",
        "||zedo.com^",
        "||bidswitch.net^",
        "||rubiconproject.com^",
        "||openx.net^",
        "||pubmatic.com^",
        "||casalemedia.com^",
        "||mediavine.com^",
        // Tracking pixels and analytics
        "||pixel.facebook.com^",
        "||bat.bing.com^",
        "||scorecardresearch.com^",
        "||chartbeat.com^",
        "||segment.io^",
        "||segment.com^",
        "||mixpanel.com^",
        "||hotjar.com^",
        "||fullstory.com^",
        "||mouseflow.com^",
        "||luckyorange.com^",
        // Service worker iframes (block as popups)
        "||googletagmanager.com/static/service_worker^",
    ];

    /// Creates a new ad blocker with starter rules
    pub fn new() -> Self {
        log::info!("Initializing ad blocker");

        Self::from_parts(Self::DEFAULT_RULES, HashMap::new(), "HiWave defaults", false, true)
    }

    /// Creates ad blocker with the given filter rules. A match names its
    /// rule (the engine keeps every rule's text).
    pub fn with_rules(rules: &[&str]) -> Self {
        log::info!("Initializing ad blocker with {} rules", rules.len());
        Self::from_parts(rules, HashMap::new(), "custom", false, true)
    }

    fn from_parts(
        rules: &[&str],
        rule_lists: HashMap<String, &'static str>,
        default_list: &'static str,
        several_lists: bool,
        names_rules: bool,
    ) -> Self {
        let engine =
            Engine::from_rules_parametrised(rules.iter().copied(), ParseOptions::default(), names_rules, true);
        Self {
            engine,
            enabled: true,
            requests_blocked: AtomicU64::new(0),
            trackers_blocked: AtomicU64::new(0),
            rule_lists,
            default_list,
            several_lists,
            names_rules,
        }
    }

    /// Creates ad blocker from several named lists: a match names the list
    /// its rule came from (the first, for a rule two lists share).
    pub fn from_named_lists(lists: &[(&'static str, &str)]) -> Self {
        Self::from_lists(lists, true)
    }

    fn from_lists(lists: &[(&'static str, &str)], names_rules: bool) -> Self {
        // One list: every match is from it, with or without rule names.
        // Several: only a match's rule text can name its list, so without
        // rule names the map would never be read (one key per list line).
        let several_lists = lists.len() > 1;
        let map = several_lists && names_rules;
        let mut rules = Vec::new();
        let mut rule_lists = HashMap::new();
        for (name, content) in lists {
            for line in content.lines() {
                if map {
                    rule_lists.entry(line.trim().to_string()).or_insert(*name);
                }
                rules.push(line);
            }
        }
        let default_list = lists.first().map(|(name, _)| *name).unwrap_or("custom");
        Self::from_parts(&rules, rule_lists, default_list, several_lists, names_rules)
    }

    /// Creates ad blocker from filter list content (e.g., EasyList)
    pub fn from_filter_list(list_content: &str) -> Self {
        log::info!("Initializing ad blocker from filter list");

        let rules: Vec<&str> = list_content.lines().collect();
        Self::with_rules(&rules)
    }

    /// Creates ad blocker with full EasyList/EasyPrivacy filter lists
    /// Falls back to default rules if filter lists can't be loaded
    pub fn with_filter_lists() -> Self {
        Self::filter_lists(false)
    }

    /// [`with_filter_lists`](Self::with_filter_lists), with a match naming
    /// its rule and list (dev-tools T2: the event log's `blocked_by`). The
    /// rule text costs about 9 MB, so only a tool that reports it asks.
    pub fn with_filter_lists_naming_rules() -> Self {
        Self::filter_lists(true)
    }

    fn filter_lists(names_rules: bool) -> Self {
        log::info!("Initializing ad blocker with EasyList/EasyPrivacy");

        match FilterListManager::new() {
            Some(manager) => {
                // Each list kept apart, so a match can name its list.
                let lists: Vec<(&'static str, String)> = FILTER_LISTS
                    .iter()
                    .filter_map(|source| manager.get_filter_list(source).map(|content| (source.name, content)))
                    .collect();
                if lists.is_empty() {
                    log::warn!("No filter lists available, using default rules");
                    Self::new()
                } else {
                    let rule_count = lists
                        .iter()
                        .flat_map(|(_, content)| content.lines())
                        .filter(|l| !l.trim().is_empty() && !l.starts_with('!'))
                        .count();
                    log::info!(
                        "Loaded {} filter rules from EasyList/EasyPrivacy",
                        rule_count
                    );
                    let named: Vec<(&'static str, &str)> =
                        lists.iter().map(|(name, content)| (*name, content.as_str())).collect();
                    Self::from_lists(&named, names_rules)
                }
            }
            None => {
                log::warn!("Failed to initialize filter list manager, using default rules");
                Self::new()
            }
        }
    }

    /// Reload rules - creates a new engine (immutable design). A match names
    /// its rule from now on, and its list is `custom`.
    pub fn load_rules(&mut self, rules: &[String]) -> HiWaveResult<()> {
        log::info!("Reloading ad blocker with {} rules", rules.len());

        let rules_refs: Vec<&str> = rules.iter().map(|s| s.as_str()).collect();
        self.engine = Engine::from_rules_debug(rules_refs, ParseOptions::default());
        self.rule_lists.clear();
        self.default_list = "custom";
        self.several_lists = false;
        self.names_rules = true;
        self.requests_blocked.store(0, Ordering::Relaxed);
        self.trackers_blocked.store(0, Ordering::Relaxed);

        Ok(())
    }

    /// Check if a request should be blocked
    pub fn should_block(&self, url: &Url, source_url: &Url, resource_type: ResourceType) -> bool {
        self.check(url, source_url, resource_type).is_some()
    }

    /// The list and rule that block a request, or `None` when none does.
    pub fn check(&self, url: &Url, source_url: &Url, resource_type: ResourceType) -> Option<BlockMatch> {
        if !self.enabled {
            return None;
        }
        // Only network requests are filtered. A `data:` (or `blob:`) URL
        // carries its own content: nothing is requested, and adblock cannot
        // build a request from it.
        if !matches!(url.scheme(), "http" | "https" | "ws" | "wss") {
            return None;
        }

        let request = adblock::request::Request::new(
            url.as_str(),
            source_url.as_str(),
            resource_type.as_str(),
        );

        match request {
            Ok(req) => {
                let result = self.engine.check_network_request(&req);

                if result.matched {
                    log::debug!("Blocked: {} (from {})", url, source_url);
                    self.requests_blocked.fetch_add(1, Ordering::Relaxed);
                    self.trackers_blocked.fetch_add(1, Ordering::Relaxed);
                    // Without rule names the engine writes "NetworkFilter".
                    let rule = result.filter.filter(|_| self.names_rules);
                    let list = self.list_of(rule.as_deref());
                    Some(BlockMatch { list: list.to_string(), rule })
                } else {
                    None
                }
            }
            Err(e) => {
                log::warn!("Failed to create request for {}: {}", url, e);
                None
            }
        }
    }

    /// The list a matched rule came from. A rule the engine joined from
    /// several (` <+> `) is looked up part by part, and named only when
    /// every part is from the same list: any of them may have matched.
    fn list_of(&self, rule: Option<&str>) -> &'static str {
        if !self.several_lists {
            return self.default_list;
        }
        if !self.names_rules {
            return UNATTRIBUTED;
        }
        let Some(rule) = rule else { return UNATTRIBUTED };
        let mut lists = rule.split(JOINED).map(|part| self.rule_lists.get(part.trim()).copied());
        match lists.next().flatten() {
            Some(list) if lists.all(|other| other == Some(list)) => list,
            _ => UNATTRIBUTED,
        }
    }

    /// Get cosmetic filters (CSS selectors to hide elements)
    pub fn get_cosmetic_filters(&self, url: &str) -> Vec<String> {
        let resources = self.engine.url_cosmetic_resources(url);
        resources.hide_selectors.into_iter().collect()
    }

    /// Enable or disable ad blocking
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        log::info!(
            "Ad blocking {}",
            if enabled { "enabled" } else { "disabled" }
        );
    }

    /// Check if ad blocking is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Get blocking statistics
    pub fn get_stats(&self) -> BlockingStats {
        BlockingStats {
            requests_blocked: self.requests_blocked.load(Ordering::Relaxed),
            bytes_saved: 0,
            trackers_blocked: self.trackers_blocked.load(Ordering::Relaxed),
        }
    }

    /// Manually increment blocking counters (for user blocklist, flood protection, etc.)
    pub fn increment_block_count(&self) {
        self.requests_blocked.fetch_add(1, Ordering::Relaxed);
        self.trackers_blocked.fetch_add(1, Ordering::Relaxed);
    }
}

impl Default for AdBlocker {
    fn default() -> Self {
        Self::new()
    }
}

/// Type of resource being requested
#[derive(Debug, Clone, Copy)]
pub enum ResourceType {
    Document,
    Script,
    Image,
    Stylesheet,
    Font,
    Xhr,
    WebSocket,
    Media,
    Other,
}

impl ResourceType {
    fn as_str(&self) -> &'static str {
        match self {
            ResourceType::Document => "document",
            ResourceType::Script => "script",
            ResourceType::Image => "image",
            ResourceType::Stylesheet => "stylesheet",
            ResourceType::Font => "font",
            ResourceType::Xhr => "xmlhttprequest",
            ResourceType::WebSocket => "websocket",
            ResourceType::Media => "media",
            ResourceType::Other => "other",
        }
    }
}

/// Statistics about blocked requests
#[derive(Debug, Clone, Default)]
pub struct BlockingStats {
    pub requests_blocked: u64,
    pub bytes_saved: u64,
    pub trackers_blocked: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_blocker() {
        let blocker = AdBlocker::new();
        assert!(blocker.is_enabled());
    }

    #[test]
    fn test_enable_disable() {
        let mut blocker = AdBlocker::new();

        blocker.set_enabled(false);
        assert!(!blocker.is_enabled());

        blocker.set_enabled(true);
        assert!(blocker.is_enabled());
    }

    #[test]
    fn test_with_rules() {
        let rules = vec!["||ads.example.com^", "||tracking.example.org^"];
        let blocker = AdBlocker::with_rules(&rules);
        assert!(blocker.is_enabled());
    }

    #[test]
    fn test_blocking() {
        let rules = vec!["||ads.example.com^"];
        let blocker = AdBlocker::with_rules(&rules);

        let url = Url::parse("https://ads.example.com/banner.js").unwrap();
        let source = Url::parse("https://example.com/").unwrap();

        let blocked = blocker.should_block(&url, &source, ResourceType::Script);
        assert!(blocked);
    }

    /// Dev-tools T2: a block names the list its rule came from and the rule
    /// as the list writes it; an allowed request names nothing.
    #[test]
    fn a_block_names_its_list_and_rule() {
        let blocker = AdBlocker::from_named_lists(&[
            ("EasyList", "! comment\n||ads.example.com^\n"),
            ("EasyPrivacy", "||pixel.example.org^$image\n"),
        ]);
        let source = Url::parse("https://example.com/").unwrap();
        let ad = Url::parse("https://ads.example.com/banner.js").unwrap();
        let pixel = Url::parse("https://pixel.example.org/p.gif").unwrap();
        let fine = Url::parse("https://example.com/app.js").unwrap();
        assert_eq!(
            blocker.check(&ad, &source, ResourceType::Script),
            Some(BlockMatch { list: "EasyList".into(), rule: Some("||ads.example.com^".into()) })
        );
        assert_eq!(
            blocker.check(&pixel, &source, ResourceType::Image),
            Some(BlockMatch { list: "EasyPrivacy".into(), rule: Some("||pixel.example.org^$image".into()) })
        );
        assert_eq!(blocker.check(&fine, &source, ResourceType::Script), None);
        assert_eq!(
            AdBlocker::new().check(&Url::parse("https://doubleclick.net/x").unwrap(), &source, ResourceType::Script),
            Some(BlockMatch { list: "HiWave defaults".into(), rule: Some("||doubleclick.net^".into()) })
        );
    }

    /// Prometheus R1 on #792: adblock-rust joins plain pattern rules with
    /// the same options into one (` <+> `), which no list writes. Such a
    /// match is named for a list only when every joined rule is from it.
    #[test]
    fn a_joined_rule_names_a_list_only_when_all_its_parts_are_from_it() {
        let source = Url::parse("https://example.com/").unwrap();
        let ad = Url::parse("https://example.com/adbanner.js").unwrap();

        // The rule that matched is EasyPrivacy's; it is joined with one of
        // EasyList's (same options, same token), so either could have matched.
        let mixed = AdBlocker::from_named_lists(&[("EasyList", "/adbanner.gif\n"), ("EasyPrivacy", "/adbanner.js\n")]);
        let found = mixed.check(&ad, &source, ResourceType::Script).expect("blocked");
        assert!(found.rule.as_deref().is_some_and(|r| r.contains(JOINED)), "the engine joined the rules: {found:?}");
        assert_eq!(found.list, UNATTRIBUTED, "{found:?}");

        // Both joined rules are EasyPrivacy's.
        let same = AdBlocker::from_named_lists(&[
            ("EasyList", "||ads.example.org^\n"),
            ("EasyPrivacy", "/adbanner.js\n/adbanner.gif\n"),
        ]);
        let found = same.check(&ad, &source, ResourceType::Script).expect("blocked");
        assert!(found.rule.as_deref().is_some_and(|r| r.contains(JOINED)), "the engine joined the rules: {found:?}");
        assert_eq!(found.list, "EasyPrivacy", "{found:?}");
    }

    /// Prometheus R1 on #792: the rule text costs every user about 9 MB, so
    /// the full lists keep it only when asked. Without it a match names no
    /// rule, and no list when several were loaded.
    #[test]
    fn without_rule_names_a_match_names_no_rule() {
        let source = Url::parse("https://example.com/").unwrap();
        let ad = Url::parse("https://ads.example.com/banner.js").unwrap();
        let lists = [("EasyList", "||ads.example.com^\n"), ("EasyPrivacy", "||pixel.example.org^\n")];
        assert_eq!(
            AdBlocker::from_lists(&lists, false).check(&ad, &source, ResourceType::Script),
            Some(BlockMatch { list: UNATTRIBUTED.into(), rule: None })
        );
        assert_eq!(
            AdBlocker::from_lists(&lists[..1], false).check(&ad, &source, ResourceType::Script),
            Some(BlockMatch { list: "EasyList".into(), rule: None })
        );
        assert_eq!(
            AdBlocker::from_lists(&lists, true).check(&ad, &source, ResourceType::Script),
            Some(BlockMatch { list: "EasyList".into(), rule: Some("||ads.example.com^".into()) })
        );
    }

    /// #792 independent review: the app's constructor (`with_filter_lists`:
    /// two lists, no rule names) built a rule-to-list map it never read, one
    /// key per list line, in each of the app's two engines.
    #[test]
    fn without_rule_names_two_lists_build_no_rule_map() {
        let source = Url::parse("https://example.com/").unwrap();
        let ad = Url::parse("https://ads.example.com/banner.js").unwrap();
        let lists = [("EasyList", "||ads.example.com^\n"), ("EasyPrivacy", "||pixel.example.org^\n")];
        let app = AdBlocker::from_lists(&lists, false);
        assert!(app.rule_lists.is_empty(), "{} keys", app.rule_lists.len());
        assert_eq!(
            app.check(&ad, &source, ResourceType::Script),
            Some(BlockMatch { list: UNATTRIBUTED.into(), rule: None })
        );
        assert_eq!(AdBlocker::from_lists(&lists, true).rule_lists.len(), 2);
    }

    /// A rule in both lists is credited to the first (`or_insert`).
    #[test]
    fn a_rule_in_both_lists_names_the_first() {
        let source = Url::parse("https://example.com/").unwrap();
        let ad = Url::parse("https://ads.example.com/banner.js").unwrap();
        let blocker = AdBlocker::from_named_lists(&[
            ("EasyList", "||ads.example.com^\n"),
            ("EasyPrivacy", "||ads.example.com^\n||pixel.example.org^\n"),
        ]);
        assert_eq!(blocker.check(&ad, &source, ResourceType::Script).map(|m| m.list), Some("EasyList".into()));
    }

    #[test]
    fn a_data_url_is_never_blocked() {
        // Even a rule that names it: a `data:` URL is not a request.
        let blocker = AdBlocker::with_rules(&["STICKY_CANARY", "*$script"]);
        let url = Url::parse("data:text/javascript,window.STICKY_CANARY=1").unwrap();
        let source = Url::parse("https://www.reddit.com/").unwrap();
        assert!(!blocker.should_block(&url, &source, ResourceType::Script));
        assert_eq!(blocker.requests_blocked.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn type_option_blocks_only_matching_resource_types() {
        // Privacy pin #364 tags every fetch with a destination so EasyList
        // `$script` / `$image` options apply. A mis-mapped type would either
        // leak tracker scripts or break first-party images.
        let blocker = AdBlocker::with_rules(&["||ads.example.com^$script"]);
        let url = Url::parse("https://ads.example.com/x").unwrap();
        let source = Url::parse("https://news.example/").unwrap();

        assert!(blocker.should_block(&url, &source, ResourceType::Script));
        assert!(
            !blocker.should_block(&url, &source, ResourceType::Image),
            "image must not match a $script rule"
        );
        assert!(
            !blocker.should_block(&url, &source, ResourceType::Document),
            "document must not match a $script rule"
        );
        assert!(!blocker.should_block(&url, &source, ResourceType::Stylesheet));
        assert!(!blocker.should_block(&url, &source, ResourceType::Font));
    }

    #[test]
    fn third_party_option_spares_first_party_requests() {
        let blocker = AdBlocker::with_rules(&["||tracker.example^$third-party"]);
        let url = Url::parse("https://tracker.example/pixel").unwrap();
        let first_party = Url::parse("https://tracker.example/page").unwrap();
        let third_party = Url::parse("https://news.example/").unwrap();

        assert!(
            !blocker.should_block(&url, &first_party, ResourceType::Image),
            "same-site request must not match $third-party"
        );
        assert!(blocker.should_block(&url, &third_party, ResourceType::Image));
    }

    #[test]
    fn default_rules_block_known_trackers_and_spare_benign_hosts() {
        let blocker = AdBlocker::new();
        let source = Url::parse("https://example.com/").unwrap();

        let tracker = Url::parse("https://doubleclick.net/pagead/js").unwrap();
        assert!(blocker.should_block(&tracker, &source, ResourceType::Script));

        let benign = Url::parse("https://example.com/app.js").unwrap();
        assert!(!blocker.should_block(&benign, &source, ResourceType::Script));
    }

    #[test]
    fn disabled_blocker_never_matches() {
        let mut blocker = AdBlocker::with_rules(&["||ads.example.com^"]);
        blocker.set_enabled(false);
        let url = Url::parse("https://ads.example.com/banner.js").unwrap();
        let source = Url::parse("https://example.com/").unwrap();
        assert!(!blocker.should_block(&url, &source, ResourceType::Script));
        assert_eq!(blocker.get_stats().requests_blocked, 0);
    }

    #[test]
    fn a_match_increments_blocking_stats() {
        let blocker = AdBlocker::with_rules(&["||ads.example.com^"]);
        let url = Url::parse("https://ads.example.com/banner.js").unwrap();
        let source = Url::parse("https://example.com/").unwrap();
        assert!(blocker.should_block(&url, &source, ResourceType::Script));
        let stats = blocker.get_stats();
        assert_eq!(stats.requests_blocked, 1);
        assert_eq!(stats.trackers_blocked, 1);
    }
}
