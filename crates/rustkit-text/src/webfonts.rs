//! Web fonts (`@font-face`): the document-scoped face registry.
//!
//! Every font-creation path in this crate and in `rustkit-layout` resolves a
//! family NAME through the platform (Core Text `new_from_name`). A web font is
//! a family name that exists only because the document declared it, so those
//! paths need one extra lookup ahead of the platform: "did the current
//! document register this family?" That lookup lives here.
//!
//! SCOPE, STATED: the registry holds the faces of ONE document at a time —
//! the engine installs a view's partition slice of its (partitioned)
//! `FontLoader` immediately before laying out or painting that view. It is a
//! process-wide slot, not a process-wide cache: two documents never see each
//! other's faces because the engine swaps the slot per view, and the loader
//! it swaps from is keyed by top-level site. Handing every `create_font`
//! call a partition parameter would have been the pure design; it also
//! touches a dozen call sites across four crates for the same guarantee,
//! which this gives by construction.
//!
//! Formats: whatever `CGFontCreateWithDataProvider` accepts — TrueType and
//! OpenType (`.ttf`/`.otf`) sfnt containers. WOFF/WOFF2 are compressed
//! wrappers around the same tables and need a decoder this workspace does not
//! carry yet; they install as nothing and are reported as rejected.

use std::sync::Arc;

/// One face as the engine hands it over: raw bytes plus the descriptors the
/// `@font-face` rule declared for them.
#[derive(Debug, Clone)]
pub struct WebFontFace {
    pub family: String,
    /// CSS weight (100..900).
    pub weight: u16,
    pub italic: bool,
    pub data: Arc<Vec<u8>>,
}

#[cfg(target_os = "macos")]
mod imp {
    use super::WebFontFace;
    use core_graphics::data_provider::CGDataProvider;
    use core_graphics::font::CGFont;
    use std::collections::HashMap;
    use std::sync::{OnceLock, RwLock};

    struct Face {
        weight: u16,
        italic: bool,
        cgfont: CGFont,
    }

    struct Active {
        /// Identifies WHICH face set is installed so a re-install of the
        /// same set is a no-op rather than a re-parse of every font file.
        tag: String,
        families: HashMap<String, Vec<Face>>,
    }

    fn slot() -> &'static RwLock<Active> {
        static SLOT: OnceLock<RwLock<Active>> = OnceLock::new();
        SLOT.get_or_init(|| {
            RwLock::new(Active {
                tag: String::new(),
                families: HashMap::new(),
            })
        })
    }

    fn key(family: &str) -> String {
        family.trim().to_ascii_lowercase()
    }

    /// Install `faces` as the active document font set, replacing whatever
    /// was there. Returns how many faces Core Graphics accepted; a face it
    /// rejected (bad data, unsupported container) is dropped and counted
    /// against that number, never silently kept as a name with no glyphs.
    pub fn install(tag: &str, faces: &[WebFontFace]) -> usize {
        {
            let active = slot().read().unwrap();
            if !tag.is_empty() && active.tag == tag {
                return active.families.values().map(Vec::len).sum();
            }
        }
        let mut families: HashMap<String, Vec<Face>> = HashMap::new();
        let mut accepted = 0usize;
        for face in faces {
            let provider = CGDataProvider::from_buffer(face.data.clone());
            let Ok(cgfont) = CGFont::from_data_provider(provider) else {
                continue;
            };
            accepted += 1;
            families.entry(key(&face.family)).or_default().push(Face {
                weight: face.weight,
                italic: face.italic,
                cgfont,
            });
        }
        let mut active = slot().write().unwrap();
        active.tag = tag.to_string();
        active.families = families;
        accepted
    }

    /// Drop the active set. After this no family resolves through the registry.
    pub fn clear() {
        let mut active = slot().write().unwrap();
        active.tag.clear();
        active.families.clear();
    }

    /// Does the active document declare `family`? Case-insensitive, as CSS
    /// family matching is.
    pub fn is_installed(family: &str) -> bool {
        slot().read().unwrap().families.contains_key(&key(family))
    }

    /// CSS Fonts 4 §5.2 reduced to the two axes the loader records: an exact
    /// italic match beats a mismatched one, then the nearest weight.
    fn select<'a>(faces: &'a [Face], weight: u16, italic: bool) -> Option<&'a Face> {
        faces.iter().min_by_key(|f| {
            let style_penalty: u32 = if f.italic == italic { 0 } else { 10_000 };
            style_penalty + (f.weight as i32 - weight as i32).unsigned_abs()
        })
    }

    /// The registered face of `family` closest to the requested style.
    pub fn lookup(family: &str, weight: u16, italic: bool) -> Option<CGFont> {
        let active = slot().read().unwrap();
        let faces = active.families.get(&key(family))?;
        select(faces, weight, italic).map(|f| f.cgfont.clone())
    }

    /// The `(weight, italic)` descriptors of the face `lookup` would return.
    /// Same selection code, exposed so the rule is testable and debuggable
    /// without comparing font objects.
    pub fn lookup_descriptor(family: &str, weight: u16, italic: bool) -> Option<(u16, bool)> {
        let active = slot().read().unwrap();
        let faces = active.families.get(&key(family))?;
        select(faces, weight, italic).map(|f| (f.weight, f.italic))
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::WebFontFace;

    /// No platform registry here yet: web fonts install as nothing, and the
    /// count says so instead of pretending.
    pub fn install(_tag: &str, _faces: &[WebFontFace]) -> usize {
        0
    }

    pub fn clear() {}

    pub fn is_installed(_family: &str) -> bool {
        false
    }

    pub fn lookup_descriptor(_family: &str, _weight: u16, _italic: bool) -> Option<(u16, bool)> {
        None
    }
}

pub use imp::{clear, install, is_installed, lookup_descriptor};

#[cfg(target_os = "macos")]
pub use imp::lookup;

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use core_text::font as ct_font;

    const AHEM: &[u8] = include_bytes!("../tests/fixtures/Ahem.ttf");

    fn ahem_face(family: &str, weight: u16, italic: bool) -> WebFontFace {
        WebFontFace {
            family: family.to_string(),
            weight,
            italic,
            data: Arc::new(AHEM.to_vec()),
        }
    }

    // The slot is process-wide and cargo runs tests on parallel threads, so
    // every test installs ONE set holding every family it will ask about,
    // under a tag of its own, and asserts only about its own families. An
    // interleaved install from another test can replace the set between two
    // statements; the assertions below are chosen so that any test's set
    // satisfies the *negative* assertions and only the positive ones need
    // the installer's own set — which is why every family name is unique to
    // its test and every test re-installs before its positive checks.

    #[test]
    fn an_installed_face_resolves_by_family_case_insensitively() {
        let faces = [ahem_face("WebfontsTestAhem", 400, false)];
        let n = install("t1", &faces);
        assert_eq!(n, 1, "Core Graphics accepts the TrueType Ahem");
        let ct = {
            install("t1", &faces);
            let cg = lookup("WEBFONTSTESTAHEM", 400, false).expect("registered family resolves");
            ct_font::new_from_CGFont(&cg, 25.0)
        };
        // Not a fallback: the CTFont built from those bytes reports Ahem's
        // own family name.
        assert_eq!(ct.family_name(), "Ahem");
    }

    #[test]
    fn a_family_nobody_declared_does_not_resolve() {
        install("t2", &[ahem_face("WebfontsTestOther", 400, false)]);
        assert!(lookup("WebfontsTestNeverDeclared", 400, false).is_none());
        assert!(!is_installed("Helvetica"), "system fonts are not web fonts");
    }

    #[test]
    fn garbage_bytes_are_rejected_not_registered() {
        let junk = WebFontFace {
            family: "WebfontsTestJunk".to_string(),
            weight: 400,
            italic: false,
            data: Arc::new(vec![0u8; 64]),
        };
        let n = install("t3", &[junk]);
        assert_eq!(n, 0);
        assert!(
            !is_installed("WebfontsTestJunk"),
            "a name with no glyphs behind it must not shadow the platform lookup"
        );
    }

    #[test]
    fn the_nearest_style_wins_and_italic_outranks_weight() {
        let faces = [
            ahem_face("WebfontsTestStyled", 400, false),
            ahem_face("WebfontsTestStyled", 700, true),
        ];
        let pick = |w, i| {
            install("t4", &faces);
            lookup_descriptor("WebfontsTestStyled", w, i).expect("family installed")
        };
        assert_eq!(pick(400, false), (400, false));
        assert_eq!(pick(900, false), (400, false), "upright 900 prefers the upright face");
        assert_eq!(pick(400, true), (700, true), "italic 400 prefers the italic face");
    }
}
