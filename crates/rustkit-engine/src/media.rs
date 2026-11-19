//! Media elements' resource loading (MEDIA-AVF Milestone 1 step 2,
//! `docs/design/MEDIA_AVF.md` §3.2-3.5): HTML §4.8.11.5's load algorithm, up to the
//! metadata. Nothing plays yet (steps 3 and 4).
//!
//! Each `<video>` / `<audio>` of a view's document is tracked by raw NodeId in its
//! [`MediaElements`] (the engine side of `docs/design/MEDIA_AVF.md` §3.2: the main-thread
//! `MediaHost` that will own players comes in step 3). The live loop syncs them with the
//! document after each layout and after `load()` calls, and an element whose resource changed (its `src`, its first
//! acceptable `<source>`, or a `load()`) runs the algorithm again:
//!
//! 1. `loadstart`. With `preload=none` the element then idles (`suspend`).
//! 2. The resource is fetched through rustkit-net, so through the shield, as one GET:
//!    step 2 holds whole resources in memory (ranged fetches come with playback). A
//!    failed or blocked fetch is `MEDIA_ERR_SRC_NOT_SUPPORTED` (Chrome 148 reports 4
//!    for a 404).
//! 3. `progress`, `suspend`, and an AVFoundation probe of the bytes
//!    (`rustkit_media::avf::Probe`), polled each turn. Not media: error 4.
//! 4. Metadata: `durationchange`, `resize` (video), `loadedmetadata` at
//!    HAVE_METADATA; then, with every byte in hand, `loadeddata`, `canplay`,
//!    `canplaythrough` at HAVE_ENOUGH_DATA.
//!
//! The state each event carries is the element's as of that event (pushed with it to
//! the getters in dom.rs). The order is Chrome 148's
//! (`docs/diagnostics/2026-11-18/media_load_cft148.json`).

use rustkit_dom::{Document, Node, NodeType};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

/// The display-list key of a video's box once its size is known (its frames paint
/// under it from step 3; nothing paints yet).
pub(crate) const VIDEO_KEY_PREFIX: &str = "rustkit-video:";

/// `MEDIA_ERR_SRC_NOT_SUPPORTED`.
const SRC_NOT_SUPPORTED: u16 = 4;

/// What a media element's getters read (pushed as JSON, dom.rs `MEDIA_INITIAL`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MediaState {
    pub(crate) ready_state: u8,
    pub(crate) network_state: u8,
    pub(crate) duration: Option<f64>,
    pub(crate) current_src: String,
    pub(crate) error: Option<u16>,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) buffered: Vec<(f64, f64)>,
    pub(crate) seekable: Vec<(f64, f64)>,
}

impl MediaState {
    fn initial() -> Self {
        MediaState {
            ready_state: 0,
            network_state: 0,
            duration: None,
            current_src: String::new(),
            error: None,
            width: 0,
            height: 0,
            buffered: Vec::new(),
            seekable: Vec::new(),
        }
    }

    pub(crate) fn json(&self) -> String {
        serde_json::json!({
            "rs": self.ready_state,
            "ns": self.network_state,
            "duration": self.duration,
            "src": self.current_src,
            "error": self.error,
            "w": self.width,
            "h": self.height,
            "buffered": self.buffered.iter().map(|(a, b)| [a, b]).collect::<Vec<_>>(),
            "seekable": self.seekable.iter().map(|(a, b)| [a, b]).collect::<Vec<_>>(),
        })
        .to_string()
    }
}

/// `preload`, as the element's resource loading reads it (HTML §4.8.11.2: missing or
/// invalid is `metadata` here, as in Chrome; empty is `auto`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Preload {
    None,
    Metadata,
    Auto,
}

impl Preload {
    fn of(value: Option<&str>) -> Preload {
        match value.map(|v| v.trim().to_ascii_lowercase()) {
            Some(v) if v == "none" => Preload::None,
            Some(v) if v.is_empty() || v == "auto" => Preload::Auto,
            _ => Preload::Metadata,
        }
    }
}

enum Stage {
    /// Nothing to load (no source), or waiting (`preload=none`).
    Idle,
    /// The fetch of this serial is in flight.
    Fetching,
    Probing(rustkit_media::avf::Probe),
    /// Metadata in, or an error: nothing more until the resource changes.
    Done,
}

/// One media element.
pub(crate) struct MediaElement {
    video: bool,
    /// The resource the element selected (absolute), with its `preload`.
    selected: Option<(String, Preload)>,
    /// Bumped each time the load algorithm runs: a fetch of an earlier run is dropped.
    serial: u64,
    stage: Stage,
    pub(crate) state: MediaState,
}

/// A fetch's outcome, for the element and run that asked.
pub(crate) struct MediaFetched {
    pub(crate) node: usize,
    pub(crate) serial: u64,
    pub(crate) bytes: Result<rustkit_media::avf::MediaBytes, ()>,
}

/// An event to fire, with the element's state as of it.
pub(crate) struct MediaEvent {
    pub(crate) node: usize,
    pub(crate) kind: &'static str,
    pub(crate) state: String,
}

/// A view's media elements.
#[derive(Default)]
pub(crate) struct MediaElements {
    elements: HashMap<usize, MediaElement>,
    events: Vec<MediaEvent>,
    /// The layout generation the document was last synced at.
    pub(crate) synced_for: Option<u64>,
}

/// What a sync asks of the engine: fetches to start (node, serial, URL).
pub(crate) struct MediaFetch {
    pub(crate) node: usize,
    pub(crate) serial: u64,
    pub(crate) url: url::Url,
}

fn is_tag(node: &Node, tag: &str) -> bool {
    node.tag_name().is_some_and(|t| t.eq_ignore_ascii_case(tag))
}

/// Every `<video>` and `<audio>` element of `document`, in tree order. The walk keeps
/// its own stack.
fn media_elements(document: &Document) -> Vec<Rc<Node>> {
    let mut out = Vec::new();
    let mut stack: Vec<Rc<Node>> = vec![document.root().clone()];
    while let Some(node) = stack.pop() {
        if matches!(node.node_type, NodeType::Element { .. })
            && (is_tag(&node, "video") || is_tag(&node, "audio"))
        {
            out.push(node.clone());
        }
        let children = node.children();
        stack.extend(children.into_iter().rev());
    }
    out
}

impl MediaElements {
    pub(crate) fn take_events(&mut self) -> Vec<MediaEvent> {
        std::mem::take(&mut self.events)
    }

    /// Whether the live loop has to keep turning for the media: a fetch or a probe is
    /// under way, or events wait.
    pub(crate) fn busy(&self) -> bool {
        !self.events.is_empty()
            || self
                .elements
                .values()
                .any(|e| matches!(e.stage, Stage::Fetching | Stage::Probing(_)))
    }

    /// Every video's display size whose metadata is in, by raw NodeId.
    pub(crate) fn video_sizes(&self) -> HashMap<usize, (u32, u32)> {
        self.elements
            .iter()
            .filter(|(_, e)| e.video && e.state.width > 0 && e.state.height > 0)
            .map(|(n, e)| (*n, (e.state.width, e.state.height)))
            .collect()
    }

    fn queue(&mut self, node: usize, kind: &'static str) {
        let state = self.elements[&node].state.json();
        self.events.push(MediaEvent { node, kind, state });
    }

    /// Bring the host in line with `document`: forget elements that left it, and run
    /// the load algorithm for each one whose selected resource changed or whose
    /// `load()` script called (`reload`). `resolve` makes a URL absolute against the
    /// document; `can_play` is `canPlayType`. Returns the fetches to start.
    pub(crate) fn sync(
        &mut self,
        document: &Document,
        reload: &[usize],
        resolve: &dyn Fn(&str) -> Option<url::Url>,
        can_play: &dyn Fn(&str) -> &'static str,
    ) -> Vec<MediaFetch> {
        let mut fetches = Vec::new();
        let mut alive = std::collections::HashSet::new();
        for node in media_elements(document) {
            let raw = node.id.raw();
            alive.insert(raw);
            let video = is_tag(&node, "video");
            let selected = Self::select(&node, resolve, can_play);
            let entry = self.elements.entry(raw).or_insert_with(|| MediaElement {
                video,
                selected: None,
                serial: 0,
                stage: Stage::Idle,
                state: MediaState::initial(),
            });
            let changed = entry.selected != selected || entry.serial == 0;
            if !(changed || reload.contains(&raw)) {
                continue;
            }
            if entry.serial == 0 && selected.is_none() {
                // Never had a resource: nothing to announce.
                entry.serial = 1;
                continue;
            }
            entry.serial += 1;
            entry.selected = selected.clone();
            entry.stage = Stage::Idle;
            entry.state = MediaState::initial();
            let Some((url, preload)) = selected else {
                continue;
            };
            entry.state.current_src = url.clone();
            if preload == Preload::None {
                // Chrome 148: `loadstart` already at NETWORK_IDLE, then `suspend`.
                entry.state.network_state = 1;
                self.queue(raw, "loadstart");
                self.queue(raw, "suspend");
                continue;
            }
            entry.state.network_state = 2;
            entry.stage = Stage::Fetching;
            let serial = entry.serial;
            self.queue(raw, "loadstart");
            match url::Url::parse(&url) {
                Ok(url) => fetches.push(MediaFetch {
                    node: raw,
                    serial,
                    url,
                }),
                Err(_) => self.fail(raw),
            }
        }
        self.elements.retain(|node, _| alive.contains(node));
        fetches
    }

    /// The element's resource (HTML §4.8.11.5, resource selection): its `src`, else the
    /// first `<source>` child with a `src` whose `type` is empty or one `canPlayType`
    /// does not reject. With the `preload` it loads under.
    fn select(
        node: &Rc<Node>,
        resolve: &dyn Fn(&str) -> Option<url::Url>,
        can_play: &dyn Fn(&str) -> &'static str,
    ) -> Option<(String, Preload)> {
        let preload = Preload::of(node.get_attribute("preload"));
        if let Some(src) = node.get_attribute("src") {
            return resolve(src).map(|u| (u.to_string(), preload));
        }
        node.children().into_iter().find_map(|child| {
            if !is_tag(&child, "source") {
                return None;
            }
            let src = child.get_attribute("src")?;
            if let Some(kind) = child.get_attribute("type") {
                if !kind.trim().is_empty() && can_play(kind).is_empty() {
                    return None;
                }
            }
            resolve(src).map(|u| (u.to_string(), preload))
        })
    }

    fn fail(&mut self, node: usize) {
        let Some(e) = self.elements.get_mut(&node) else {
            return;
        };
        e.stage = Stage::Done;
        e.state.network_state = 3;
        e.state.error = Some(SRC_NOT_SUPPORTED);
        self.queue(node, "error");
    }

    /// A fetch came back.
    pub(crate) fn fetched(&mut self, fetched: MediaFetched) {
        let Some(e) = self.elements.get_mut(&fetched.node) else {
            return;
        };
        if e.serial != fetched.serial || !matches!(e.stage, Stage::Fetching) {
            return;
        }
        match fetched.bytes {
            Ok(bytes) => {
                e.state.network_state = 1;
                e.stage = Stage::Probing(rustkit_media::avf::Probe::start(Arc::new(bytes)));
                self.queue(fetched.node, "progress");
                self.queue(fetched.node, "suspend");
            }
            Err(()) => {
                // A 404 or a blocked request: Chrome 148 fires `error` with no
                // `progress` (code 4).
                self.fail(fetched.node);
            }
        }
    }

    /// Poll the probes. Returns whether a video's size changed (its box must be laid
    /// out again).
    pub(crate) fn poll(&mut self) -> bool {
        let mut resized = false;
        let nodes: Vec<usize> = self
            .elements
            .iter()
            .filter(|(_, e)| matches!(e.stage, Stage::Probing(_)))
            .map(|(n, _)| *n)
            .collect();
        for node in nodes {
            let e = self.elements.get_mut(&node).expect("listed");
            let Stage::Probing(probe) = &mut e.stage else {
                continue;
            };
            match probe.poll() {
                rustkit_media::avf::ProbeState::Pending => {}
                rustkit_media::avf::ProbeState::Failed => self.fail(node),
                rustkit_media::avf::ProbeState::Ready {
                    duration, video, ..
                } => {
                    e.stage = Stage::Done;
                    e.state.ready_state = 1;
                    e.state.duration = Some(duration);
                    e.state.buffered = vec![(0.0, duration)];
                    e.state.seekable = vec![(0.0, duration)];
                    let sized = match (e.video, video) {
                        (true, Some((w, h))) if w > 0.0 && h > 0.0 => {
                            e.state.width = w.round() as u32;
                            e.state.height = h.round() as u32;
                            true
                        }
                        _ => false,
                    };
                    self.queue(node, "durationchange");
                    if sized {
                        resized = true;
                        self.queue(node, "resize");
                    }
                    self.queue(node, "loadedmetadata");
                    // Every byte is in hand: the data is all there.
                    self.elements
                        .get_mut(&node)
                        .expect("listed")
                        .state
                        .ready_state = 4;
                    self.queue(node, "loadeddata");
                    self.queue(node, "canplay");
                    self.queue(node, "canplaythrough");
                }
            }
        }
        resized
    }
}

/// `canPlayType` (HTML §4.8.11.3) from AVFoundation: `"probably"` for a type with
/// `codecs` AVFoundation plays, `"maybe"` for a container type it plays, `""`
/// otherwise. Chrome 148's answers for the pinned strings are in
/// `docs/diagnostics/2026-11-18/media_codecs_cft148.json`; RustKit differs where
/// AVFoundation does (WebM / VP9), and for HLS (`application/vnd.apple.mpegurl`),
/// which Pete kept out of Milestone 1.
pub(crate) fn can_play_type(mime: &str) -> &'static str {
    let mime = mime.trim();
    let essence = mime
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if essence.is_empty()
        || essence.contains("mpegurl")
        || !rustkit_media::avf::is_playable_extended_mime_type(mime)
    {
        return "";
    }
    if mime.to_ascii_lowercase().contains("codecs=") {
        "probably"
    } else {
        "maybe"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preload_reads_as_chrome() {
        assert_eq!(Preload::of(None), Preload::Metadata);
        assert_eq!(Preload::of(Some("")), Preload::Auto);
        assert_eq!(Preload::of(Some("AUTO")), Preload::Auto);
        assert_eq!(Preload::of(Some("none")), Preload::None);
        assert_eq!(Preload::of(Some("bogus")), Preload::Metadata);
    }
}
