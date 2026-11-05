//! Edit commands for script and for the user's shortcuts (CH2.E1):
//! `document.execCommand`/`queryCommandSupported`, `navigator.clipboard`,
//! and [`crate::DomBindings::user_edit`], which fires `beforeinput` and
//! `input` around a command the engine's key mapping chose.
//!
//! Every command runs on rustkit-dom's `TextEditState` (the same model the
//! engine's keyboard path drives): the engine shares its per-view map of
//! edit states (`DomBindings::share_edit_states`), so a script's
//! `execCommand('undo')` undoes what the user typed. Without an engine the
//! bindings keep a map of their own.

use rustkit_dom::clipboard::{MemoryClipboard, SharedClipboard};
use rustkit_dom::forms::TextEditState;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

/// A view's text-control edit states, by raw NodeId.
pub type SharedEditStates = Rc<RefCell<HashMap<usize, TextEditState>>>;

/// What the edit host functions share with `DomBindings`.
pub struct EditContext {
    pub(crate) clipboard: RefCell<SharedClipboard>,
    pub(crate) states: RefCell<SharedEditStates>,
    /// Whether script may read the clipboard (`readText`,
    /// `execCommand('paste')`). A user's paste is always allowed.
    pub(crate) script_read: Cell<bool>,
}

impl Default for EditContext {
    fn default() -> Self {
        Self {
            clipboard: RefCell::new(MemoryClipboard::shared()),
            states: RefCell::new(Rc::default()),
            script_read: Cell::new(true),
        }
    }
}
