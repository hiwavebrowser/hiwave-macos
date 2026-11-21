//! Portable `console_file::setting` pins. The backslash-as-separator rule
//! (#1315 / batch 106) lives in headless `console_file_tests`; Linux default
//! `--lib` never listed it. Pure: no Engine, no file, no env.

use crate::console_file::{setting, Setting};
use std::ffi::OsStr;
use std::path::PathBuf;

#[test]
fn only_a_value_with_a_separator_names_a_file() {
    for off in ["", "0", "1", "true", "on"] {
        assert_eq!(setting(OsStr::new(off)), Setting::Off, "{off:?}");
    }
    assert_eq!(
        setting(OsStr::new("/tmp/c.jsonl")),
        Setting::File(PathBuf::from("/tmp/c.jsonl"))
    );
    assert_eq!(
        setting(OsStr::new("./c.jsonl")),
        Setting::File(PathBuf::from("./c.jsonl"))
    );
    // Windows paths are written with `\`; without this they were NotAPath /
    // Off and HIWAVE_CONSOLE_LOG did nothing on that OS.
    for path in [r"C:\logs\c.jsonl", r".\c.jsonl"] {
        assert_eq!(
            setting(OsStr::new(path)),
            Setting::File(PathBuf::from(path)),
            "{path:?}"
        );
    }
}

#[test]
fn a_bare_punctuation_value_is_not_a_path() {
    for wrong in ["?", "*", "c.jsonl", "console"] {
        // alphanumeric-only bare words are Off; other non-separator values
        // are NotAPath (the one-shot warning path).
        let got = setting(OsStr::new(wrong));
        assert!(
            matches!(got, Setting::Off | Setting::NotAPath),
            "{wrong:?} → {got:?}"
        );
        assert!(
            !matches!(got, Setting::File(_)),
            "{wrong:?} must not name a file"
        );
    }
}
