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
    /// the starter rules, or `custom` for rules handed to `with_rules`.
    pub list: String,
    /// The rule as the list writes it.
    pub rule: Option<String>,
}

/// Ad blocker powered by Brave's adblock-rust engine
pub struct AdBlocker {
    engine: Engine,
    enabled: bool,
    requests_blocked: AtomicU64,
    trackers_blocked: AtomicU64,
    /// The list each rule came from, when rules came from more than one.
    /// The engine is built with debug information, so a match names its
    /// rule's text (a few MB more for the full lists).
    rule_lists: HashMap<String, &'static str>,
    /// The list a rule not in `rule_lists` came from.
    default_list: &'static str,
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

        Self::from_parts(Self::DEFAULT_RULES, HashMap::new(), "HiWave defaults")
    }

    /// Creates ad blocker with the given filter rules
    pub fn with_rules(rules: &[&str]) -> Self {
        log::info!("Initializing ad blocker with {} rules", rules.len());
        Self::from_parts(rules, HashMap::new(), "custom")
    }

    fn from_parts(rules: &[&str], rule_lists: HashMap<String, &'static str>, default_list: &'static str) -> Self {
        let engine = Engine::from_rules_debug(rules.iter().copied(), ParseOptions::default());
        Self {
            engine,
            enabled: true,
            requests_blocked: AtomicU64::new(0),
            trackers_blocked: AtomicU64::new(0),
            rule_lists,
            default_list,
        }
    }

    /// Creates ad blocker from several named lists: a match names the list
    /// its rule came from (the first, for a rule two lists share).
    pub fn from_named_lists(lists: &[(&'static str, &str)]) -> Self {
        let mut rules = Vec::new();
        let mut rule_lists = HashMap::new();
        for (name, content) in lists {
            for line in content.lines() {
                rule_lists.entry(line.trim().to_string()).or_insert(*name);
                rules.push(line);
            }
        }
        let default_list = lists.first().map(|(name, _)| *name).unwrap_or("custom");
        Self::from_parts(&rules, rule_lists, default_list)
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
                    Self::from_named_lists(&named)
                }
            }
            None => {
                log::warn!("Failed to initialize filter list manager, using default rules");
                Self::new()
            }
        }
    }

    /// Reload rules - creates a new engine (immutable design)
    pub fn load_rules(&mut self, rules: &[String]) -> HiWaveResult<()> {
        log::info!("Reloading ad blocker with {} rules", rules.len());

        let rules_refs: Vec<&str> = rules.iter().map(|s| s.as_str()).collect();
        self.engine = Engine::from_rules_debug(rules_refs, ParseOptions::default());
        self.rule_lists.clear();
        self.default_list = "custom";
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
                    let rule = result.filter;
                    let list = rule
                        .as_deref()
                        .and_then(|r| self.rule_lists.get(r.trim()).copied())
                        .unwrap_or(self.default_list);
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
