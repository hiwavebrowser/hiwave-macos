//! macOS text input (CH2.E3): marked text (a dead key or an IME composing),
//! commits, the Edit menu's commands and Cmd chords, as the content view's
//! `NSTextInputClient` and responder methods hand them to the page.

use crate::{Engine, EngineViewId};

/// A focused text control's text-input state, for the platform's input
/// client: what is marked and selected (UTF-16 offsets into the value) and
/// where the caret is painted (viewport CSS px), so an IME's candidate
/// window sits at the caret.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TextInputState {
    pub marked: Option<(usize, usize)>,
    pub selected: (usize, usize),
    pub caret: Option<TextInputCaret>,
}

/// The caret's rectangle in viewport CSS px.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TextInputCaret {
    pub x: f32,
    pub y: f32,
    pub height: f32,
}

impl Engine {
    /// `setMarkedText:`. Not implemented yet (red).
    pub fn set_marked_text(&mut self, _id: EngineViewId, _text: &str) -> bool {
        false
    }

    /// `insertText:`. Not implemented yet (red).
    pub fn insert_text(&mut self, _id: EngineViewId, _text: &str) -> bool {
        false
    }

    /// `unmarkText`. Not implemented yet (red).
    pub fn unmark_text(&mut self, _id: EngineViewId) -> bool {
        false
    }

    /// Not implemented yet (red).
    pub fn text_input_state(&self, _id: EngineViewId) -> Option<TextInputState> {
        None
    }

    /// Not implemented yet (red).
    pub fn handle_command_key(&mut self, _id: EngineViewId, _key_code: u32, _key: &str, _shift: bool, _alt: bool) -> bool {
        false
    }

    /// Not implemented yet (red).
    pub fn run_edit_command(&mut self, _id: EngineViewId, _command: &rustkit_dom::forms::EditCommand) -> bool {
        false
    }
}
