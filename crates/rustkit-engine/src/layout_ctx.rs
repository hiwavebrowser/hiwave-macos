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

use crate::{EngineViewId, StyleRecord};

#[derive(Default)]
pub(crate) struct LayoutCtx {
    /// Cascade provenance, recorded by the REAL cascade when armed.
    ///
    /// `None` = not recording, which is the default and costs nothing. See
    /// [`crate::Engine::set_style_recording`] for why this is a side table
    /// rather than a field on `ComputedStyle`.
    pub(crate) style_trace: RefCell<Option<Vec<StyleRecord>>>,
    /// View currently being laid out, so the view-agnostic recursive builder
    /// can read that view's edit states (and only that view's).
    pub(crate) building_view: Cell<Option<EngineViewId>>,
    /// Focused node of the view currently being laid out.
    ///
    /// Focus is per-VIEW but the recursive layout builder is view-agnostic,
    /// so the alternative was checking "is this node focused in ANY view",
    /// which would mark a node focused in one view while building another.
    /// Set by `relayout` around the build; `None` outside one.
    pub(crate) building_focus: Cell<Option<rustkit_dom::NodeId>>,
    /// Parsed SVG documents keyed by URL. SVGs referenced from <img> are
    /// vector content — they bypass ImageManager's raster decode (which
    /// rejects them as "Unknown image format") and are spliced into the
    /// display list as vector commands at build time.
    pub(crate) svg_cache: RefCell<HashMap<String, rustkit_svg::SvgDocument>>,
}
