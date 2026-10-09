//! Why a view was laid out again: the cause each `relayout` records and
//! logs, and the per-view tallies since the last navigation.
//!
//! Diagnostics only. Nothing here decides whether a relayout happens: the
//! call sites that lay out note a cause first, and `Engine::relayout` takes
//! what was noted, logs one line and counts it.

use rustkit_bindings::{MutationKind, ScriptMutations};
use std::fmt;

/// What asked for a relayout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayoutCause {
    /// The first layout of a newly committed document (`load_url`,
    /// `load_html`), deferred or not.
    InitialLoad,
    /// External stylesheets arrived (or the previous document's were
    /// cleared).
    StylesheetLoaded,
    /// Images arrived: `count` of them. Whether a decoded size changed any
    /// box is not known here (the box's size at the previous build is not
    /// kept).
    ImageLoaded { count: usize },
    /// Web fonts arrived.
    FontLoaded,
    /// Script wrote to the DOM. `kind` and `node` (`tag#id`, or just the tag
    /// when it has no id; empty for a text node) are the first write's.
    ScriptMutation { kind: MutationKind, node: String },
    /// The user scrolled and the page's scroll listeners (or the observers
    /// the scroll ticked) changed the DOM.
    Scroll,
    /// The view's bounds changed.
    Resize,
    /// A live turn ran the page's timers, network callbacks or modules and
    /// they changed the DOM.
    Timer,
    /// An IntersectionObserver or ResizeObserver callback changed the DOM
    /// after a layout.
    ObserverCallback,
    /// The hovered, pressed or focused element changed and a sheet reads it,
    /// or a pointer-move or focus event's listeners changed the DOM.
    HoverOrFocus,
    /// A click, mouse, key or form-submit event's listeners changed the DOM,
    /// or an edit changed a control's value.
    Input,
    /// Anything else, named. The name is also the key it is counted under,
    /// so it must come from a fixed set of literals, never be built at run
    /// time.
    Other(&'static str),
}

impl RelayoutCause {
    /// The snake_case kind name the log and the counters use.
    pub fn kind(&self) -> &'static str {
        match self {
            RelayoutCause::InitialLoad => "initial_load",
            RelayoutCause::StylesheetLoaded => "stylesheet_loaded",
            RelayoutCause::ImageLoaded { .. } => "image_loaded",
            RelayoutCause::FontLoaded => "font_loaded",
            RelayoutCause::ScriptMutation { .. } => "script_mutation",
            RelayoutCause::Scroll => "scroll",
            RelayoutCause::Resize => "resize",
            RelayoutCause::Timer => "timer",
            RelayoutCause::ObserverCallback => "observer_callback",
            RelayoutCause::HoverOrFocus => "hover_or_focus",
            RelayoutCause::Input => "input",
            RelayoutCause::Other(name) => name,
        }
    }
}

impl fmt::Display for RelayoutCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RelayoutCause::ImageLoaded { count } => write!(f, "image_loaded(count={count})"),
            RelayoutCause::ScriptMutation { kind, node } if node.is_empty() => {
                write!(f, "script_mutation({})", kind.as_str())
            }
            RelayoutCause::ScriptMutation { kind, node } => {
                write!(f, "script_mutation({}@{node})", kind.as_str())
            }
            RelayoutCause::Other(name) => write!(f, "other({name})"),
            cause => f.write_str(cause.kind()),
        }
    }
}

/// Counts by name, in first-seen order (a handful of entries at most).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CauseCounts(Vec<(&'static str, u32)>);

impl CauseCounts {
    pub fn add(&mut self, name: &'static str, n: u32) {
        match self.0.iter_mut().find(|(k, _)| *k == name) {
            Some((_, count)) => *count = count.saturating_add(n),
            None => self.0.push((name, n)),
        }
    }

    pub fn get(&self, name: &str) -> u32 {
        self.0
            .iter()
            .find(|(k, _)| *k == name)
            .map_or(0, |(_, n)| *n)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&'static str, u32)> + '_ {
        self.0.iter().copied()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for CauseCounts {
    /// `{script_mutation:3,image_loaded:1}`
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("{")?;
        for (i, (name, n)) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str(",")?;
            }
            write!(f, "{name}:{n}")?;
        }
        f.write_str("}")
    }
}

/// The causes noted on a view since its last relayout: the first, and a
/// count per kind. Script writes count once each under `script_mutation`,
/// with their own kinds in `mutations`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PendingCauses {
    pub first: Option<RelayoutCause>,
    pub counts: CauseCounts,
    pub mutations: CauseCounts,
}

impl PendingCauses {
    pub fn note(&mut self, cause: RelayoutCause) {
        self.counts.add(cause.kind(), 1);
        if self.first.is_none() {
            self.first = Some(cause);
        }
    }

    /// Note script's DOM writes since the last take, under `trigger` (what
    /// ran the script) when there is one. Writes the tally does not see (an
    /// embedder `mark_dirty`) leave only the trigger.
    pub fn note_script(&mut self, trigger: Option<RelayoutCause>, writes: ScriptMutations) {
        if let Some(trigger) = trigger {
            self.note(trigger);
        }
        let Some((kind, node)) = writes.first.clone() else {
            return;
        };
        self.counts.add("script_mutation", writes.total());
        for kind in MutationKind::ALL {
            let n = writes.count(kind);
            if n > 0 {
                self.mutations.add(kind.as_str(), n);
            }
        }
        if self.first.is_none() {
            self.first = Some(RelayoutCause::ScriptMutation { kind, node });
        }
    }

    pub fn is_empty(&self) -> bool {
        self.first.is_none()
    }
}

/// A view's relayouts since its current document was committed, by cause.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RelayoutStats {
    /// Relayouts that completed.
    pub relayouts: u32,
    /// Relayouts by their first cause's kind (these sum to `relayouts`).
    pub by_first_cause: CauseCounts,
    /// Every cause noted, summed over all relayouts.
    pub by_cause: CauseCounts,
    /// Script DOM writes behind the relayouts, by mutation kind.
    pub mutations: CauseCounts,
    /// Relayouts whose build took the kept box tree.
    pub trees_reused: u32,
    /// Time spent in them, layout and the paint it ends with.
    pub total_ms: f64,
    /// The most recent relayout's first cause.
    pub last: Option<RelayoutCause>,
}

impl RelayoutStats {
    pub fn record(&mut self, causes: &PendingCauses, tree_reused: bool, took_ms: f64) {
        self.relayouts = self.relayouts.saturating_add(1);
        if let Some(first) = &causes.first {
            self.by_first_cause.add(first.kind(), 1);
        }
        for (name, n) in causes.counts.iter() {
            self.by_cause.add(name, n);
        }
        for (name, n) in causes.mutations.iter() {
            self.mutations.add(name, n);
        }
        if tree_reused {
            self.trees_reused = self.trees_reused.saturating_add(1);
        }
        self.total_ms += took_ms;
        self.last = causes.first.clone();
    }
}

/// The document URL as the navigation summary logs it: origin and path for
/// a network URL, never its query or fragment (they can carry tokens).
/// `about:` keeps the first 32 characters of its path; any other scheme
/// (`file:`, `data:`, `blob:`, ...) logs the scheme alone, since its path
/// can be a local path, a whole document or another URL.
pub(crate) fn summary_url(url: &url::Url) -> String {
    match url.scheme() {
        "http" | "https" | "ws" | "wss" => {
            format!("{}{}", url.origin().ascii_serialization(), url.path())
        }
        "about" => {
            let path: String = url.path().chars().take(32).collect();
            format!("about:{path}")
        }
        scheme => format!("{scheme}:"),
    }
}

thread_local! {
    /// Whether the last `build_layout_from_document` on this thread took the
    /// kept box tree instead of walking the DOM. The engine is single
    /// threaded; `relayout` reads it right after its own build.
    static LAST_BUILD_REUSED_TREE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub(crate) fn set_last_build_reused_tree(reused: bool) {
    LAST_BUILD_REUSED_TREE.with(|c| c.set(reused));
}

pub(crate) fn last_build_reused_tree() -> bool {
    LAST_BUILD_REUSED_TREE.with(|c| c.get())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_summary_url_drops_the_query_and_fragment() {
        let url = |s: &str| summary_url(&url::Url::parse(s).unwrap());
        assert_eq!(
            url("https://example.com:8443/a/b?token=secret#frag"),
            "https://example.com:8443/a/b"
        );
        assert_eq!(url("about:blank"), "about:blank");
        assert_eq!(
            url("data:text/html,<p>a long inline document body here</p>"),
            "data:"
        );
        assert_eq!(url("file:///Users/someone/page.html"), "file:");
        assert_eq!(url("blob:https://a.example/0b5e-uuid"), "blob:");
    }

    #[test]
    fn first_cause_is_kept_and_kinds_are_counted() {
        let mut p = PendingCauses::default();
        p.note(RelayoutCause::HoverOrFocus);
        p.note(RelayoutCause::ImageLoaded { count: 2 });
        p.note(RelayoutCause::HoverOrFocus);
        assert_eq!(p.first, Some(RelayoutCause::HoverOrFocus));
        assert_eq!(p.counts.to_string(), "{hover_or_focus:2,image_loaded:1}");
    }

    #[test]
    fn script_writes_count_each_and_name_the_first() {
        let mut p = PendingCauses::default();
        p.note_script(None, ScriptMutations::default());
        assert!(p.is_empty(), "no trigger and no writes notes nothing");
        let mut w = ScriptMutations::default();
        w.record(MutationKind::Attribute, || "div#main".into());
        w.record(MutationKind::ChildList, || {
            unreachable!("only the first is named")
        });
        w.record(MutationKind::Attribute, || unreachable!());
        p.note_script(None, w.clone());
        assert_eq!(
            p.first,
            Some(RelayoutCause::ScriptMutation {
                kind: MutationKind::Attribute,
                node: "div#main".into()
            })
        );
        assert_eq!(p.counts.to_string(), "{script_mutation:3}");
        assert_eq!(p.mutations.to_string(), "{attribute:2,child_list:1}");

        let mut q = PendingCauses::default();
        q.note_script(Some(RelayoutCause::Scroll), w);
        assert_eq!(
            q.first,
            Some(RelayoutCause::Scroll),
            "the trigger comes first"
        );
        assert_eq!(q.counts.to_string(), "{scroll:1,script_mutation:3}");
    }

    #[test]
    fn causes_display_with_their_detail() {
        let shown = [
            RelayoutCause::InitialLoad.to_string(),
            RelayoutCause::ImageLoaded { count: 3 }.to_string(),
            RelayoutCause::ScriptMutation {
                kind: MutationKind::Style,
                node: "p#x".into(),
            }
            .to_string(),
            RelayoutCause::ScriptMutation {
                kind: MutationKind::Text,
                node: String::new(),
            }
            .to_string(),
            RelayoutCause::Other("embedder").to_string(),
        ];
        assert_eq!(
            shown,
            [
                "initial_load",
                "image_loaded(count=3)",
                "script_mutation(style@p#x)",
                "script_mutation(text)",
                "other(embedder)",
            ]
        );
    }

    #[test]
    fn stats_sum_by_first_cause_and_by_every_cause() {
        let mut stats = RelayoutStats::default();
        let mut a = PendingCauses::default();
        a.note(RelayoutCause::InitialLoad);
        stats.record(&a, false, 2.0);
        let mut b = PendingCauses::default();
        b.note(RelayoutCause::Timer);
        let mut w = ScriptMutations::default();
        w.record(MutationKind::Text, String::new);
        b.note_script(None, w);
        stats.record(&b, true, 1.5);
        assert_eq!(stats.relayouts, 2);
        assert_eq!(stats.by_first_cause.to_string(), "{initial_load:1,timer:1}");
        assert_eq!(
            stats.by_cause.to_string(),
            "{initial_load:1,timer:1,script_mutation:1}"
        );
        assert_eq!(stats.mutations.to_string(), "{text:1}");
        assert_eq!(stats.trees_reused, 1);
        assert_eq!(stats.total_ms, 3.5);
    }
}
