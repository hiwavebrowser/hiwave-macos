//! What the style and layout build reads besides the view and the
//! document, behind one `Rc` (forced layout, Pete's option 2, 2026-10-26).
//!
//! A script's geometry read inside a task has to lay the page out again
//! (forced layout). Script runs with the view's bindings borrowed out of
//! the engine, so the read cannot reach `&mut Engine`. The build's state
//! therefore moves out of `Engine` into a context the bindings' closures
//! can hold, the way they hold the hit tester and the selector matcher.
//! This is the first step: the engine-local build state. The engine keeps
//! one context and reads it through `Engine::lctx`; nothing else changes.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use url::Url;

use rustkit_image::ImageManager;
use rustkit_layout::FontLoader;

use crate::{EngineViewId, StyleRecord};

pub(crate) struct LayoutCtx {
    /// Decoded images: the build reads natural sizes from it (and decodes a
    /// `data:` image in place).
    pub(crate) image_manager: Arc<ImageManager>,
    /// Webfont loader. PER-ENGINE AND Arc-SHARED per the 2026-08-08 design
    /// pin: one loader for the whole engine, with cross-site isolation
    /// provided by the partition key on every operation rather than by
    /// handing each view its own object. Per-view loaders would refetch the
    /// same face for every tab; an unpartitioned shared one would be a
    /// cross-site timing oracle. The partitioned shared loader is the only
    /// shape that is both.
    pub(crate) font_loader: Arc<FontLoader>,
    /// Cascade provenance, recorded by the REAL cascade when armed.
    ///
    /// `None` = not recording, which is the default and costs nothing. See
    /// [`crate::Engine::set_style_recording`] for why this is a side table
    /// rather than a field on `ComputedStyle`.
    pub(crate) style_trace: RefCell<Option<Vec<StyleRecord>>>,
    /// The view currently being laid out, so the view-agnostic recursive
    /// builder can read that view's edit states, focus and viewport (and
    /// only that view's). Set by `build_layout_for_view` around the build;
    /// `None` outside one.
    pub(crate) building: RefCell<Option<Rc<BuildView>>>,
    /// Parsed SVG documents keyed by URL. SVGs referenced from <img> are
    /// vector content — they bypass ImageManager's raster decode (which
    /// rejects them as "Unknown image format") and are spliced into the
    /// display list as vector commands at build time.
    pub(crate) svg_cache: RefCell<HashMap<String, rustkit_svg::SvgDocument>>,
}

impl Default for LayoutCtx {
    fn default() -> Self {
        Self {
            image_manager: Arc::new(ImageManager::new()),
            font_loader: Arc::new(FontLoader::new()),
            style_trace: RefCell::default(),
            building: RefCell::default(),
            svg_cache: RefCell::default(),
        }
    }
}

/// What a build reads of the view it is for, taken by the engine when the
/// build starts.
pub(crate) struct BuildView {
    pub(crate) id: EngineViewId,
    /// `None` when the view is not (or no longer) in the engine.
    pub(crate) lay: Option<Rc<ViewLayout>>,
    /// The document's URL: relative resource URLs and the font partition.
    pub(crate) url: Option<Url>,
    /// The viewport in CSS px: the headless bounds, else the host surface.
    pub(crate) viewport: Option<(f32, f32)>,
    /// Focus is per-VIEW but the recursive layout builder is view-agnostic,
    /// so the alternative was checking "is this node focused in ANY view",
    /// which would mark a node focused in one view while building another.
    pub(crate) focus: Option<rustkit_dom::NodeId>,
    /// The view's `<link>` sheets: the rule-index fingerprint vouches for
    /// them only when the build was handed this very list.
    pub(crate) external_sheets: Rc<Vec<rustkit_css::Stylesheet>>,
}

impl LayoutCtx {
    pub(crate) fn building_id(&self) -> Option<EngineViewId> {
        self.building.borrow().as_ref().map(|b| b.id)
    }

    pub(crate) fn building_lay(&self) -> Option<Rc<ViewLayout>> {
        self.building.borrow().as_ref().and_then(|b| b.lay.clone())
    }

    pub(crate) fn building_focus(&self) -> Option<rustkit_dom::NodeId> {
        self.building.borrow().as_ref().and_then(|b| b.focus)
    }

    pub(crate) fn building_viewport(&self) -> Option<(f32, f32)> {
        self.building.borrow().as_ref().and_then(|b| b.viewport)
    }

    pub(crate) fn building_url(&self) -> Option<Url> {
        self.building.borrow().as_ref().and_then(|b| b.url.clone())
    }
}

/// What the build reads and writes of one view, shared the same way
/// (`ViewState::lay`). The engine owns the writes outside a build; a build
/// only reads, apart from the three outputs at the end.
#[derive(Default)]
pub(crate) struct ViewLayout {
    /// Currently focused DOM node.
    pub(crate) focused_node: Cell<Option<rustkit_dom::NodeId>>,
    /// The element the pointer is over and the elements above it (raw
    /// NodeIds): what `:hover` matches in this view's next build.
    pub(crate) hover_chain: RefCell<Vec<usize>>,
    /// The element the press in progress landed on and the elements above
    /// it: what `:active` matches, from the press to the release.
    pub(crate) active_chain: RefCell<Vec<usize>>,
    /// Live text-editing state for this view's form controls, keyed by raw
    /// `NodeId`.
    ///
    /// Per-VIEW, not engine-global: `NodeId` is per-DOCUMENT (every
    /// `Document` restarts its counter at 1), so a global map lets one
    /// document's node 4 collide with another's. Living on the view also
    /// makes the lifetime obvious — drop the view, drop the map — and gives
    /// document replacement one clear place to clear.
    ///
    /// This is the IDL-value side table browsers keep separate from the
    /// content attribute: `getAttribute("value")` is the authored default,
    /// this is the live value. Mutating the DOM instead would conflate the
    /// two and break form-reset semantics.
    pub(crate) edit_states: RefCell<HashMap<usize, rustkit_dom::forms::TextEditState>>,
    /// Checkedness of the checkboxes and radio buttons a click or script
    /// has set, keyed by raw NodeId: the other half of the same side table.
    /// A control missing here is checked when it has the `checked`
    /// attribute. Script owns the state (`input.checked`); style
    /// (`:checked`), paint and form submission read it here.
    pub(crate) checked_states: RefCell<HashMap<usize, bool>>,
    /// The parsed `<style>` sheets of this view's document, by text.
    pub(crate) sheet_cache: RefCell<crate::SheetCache>,
    /// The rule index this view was last built with: its `reads` say
    /// which elements a `:hover` or `:active` rule can match, so which
    /// changes of the two chains are a restyle.
    pub(crate) rule_reads: RefCell<Option<std::rc::Rc<crate::RuleIndex>>>,
    /// Whether `html` had a non-visible `overflow` at the last build. The
    /// viewport then takes html's, and a body with its own is a scroller
    /// that clips what it holds (see `scrollable_bottom`).
    pub(crate) html_clips: Cell<bool>,
}
