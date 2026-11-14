//! Optional copy of the page's console into a file of its own. Off unless
//! `HIWAVE_CONSOLE_LOG=<path>` (or `parity-capture --console-log <path>`).
//!
//! Stub: the switch is read, nothing is written yet.

use std::path::PathBuf;

/// The file stops growing here, with one last `truncated` line.
pub const MAX_FILE_BYTES: u64 = 50 * 1024 * 1024;
/// A message longer than this many bytes is cut (at a character).
pub const MAX_MESSAGE_BYTES: usize = 8 * 1024;

/// Write the page console to `path` for the rest of the process (the
/// command-line form of `HIWAVE_CONSOLE_LOG=<path>`). Call it before the
/// first engine is made.
pub fn set_path(_path: impl Into<PathBuf>) {}

/// What a test forces on its own thread.
#[cfg(test)]
pub(crate) enum Forced {
    Off,
    /// Write to this file, and stop at this many bytes.
    On(PathBuf, u64),
}

#[cfg(test)]
pub(crate) fn force_for_test(_forced: Option<Forced>) {}

/// The file a `HIWAVE_CONSOLE_LOG` value names, if it names one.
pub(crate) fn path_from(_value: &str) -> Option<PathBuf> {
    None
}
