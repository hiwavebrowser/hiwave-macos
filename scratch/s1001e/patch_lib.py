"""rustkit-layout lib.rs: the Text command carries the frozen run.
usage: patch_lib.py <crates/rustkit-layout/src>"""
import sys

root = sys.argv[1]


def patch(path, pairs):
    s = open(path).read()
    for old, new, count in pairs:
        assert s.count(old) == count, (path, s.count(old), old[:60])
        s = s.replace(old, new)
    open(path, "w").write(s)


lib = [
    (
        """    FontFamilyChain, FontLoader, LineHeight, PositionedGlyph, ShapedRun, TextDecoration, TextError,
    TextMetrics, TextShaper, TopLevelSite, TEXT_METRICS_ARE_FONT_DERIVED, TEXT_SHAPER_BACKEND,
};
""",
        """    FontFamilyChain, FontLoader, LineHeight, PositionedGlyph, ShapedRun, TextDecoration, TextError,
    TextMetrics, TextShaper, TopLevelSite, TEXT_METRICS_ARE_FONT_DERIVED, TEXT_SHAPER_BACKEND,
};
pub use text::{FaceIdentity, FaceSynthesis, GlyphRun, RunGlyph};
""",
        1,
    ),
    (
        """        /// third per-glyph shaper.
        ascent: Option<f32>,
    },
    /// Draw text decoration line (underline, strikethrough, overline).
""",
        """        /// third per-glyph shaper.
        ascent: Option<f32>,
        /// SHAPED-RUN CONTRACT, slice S0
        /// (docs/SHAPED_RUN_CONTRACT_2026-09-30.md): the frozen run layout
        /// shaped for this line. When present, paint places ITS glyph ids
        /// from ITS face and resolves no family list; `advances` is then
        /// this run's per-character projection and `font_family` is kept
        /// for the old path only. `None` where the run is outside the slice
        /// (a fallback character, an emoji, a platform whose shaper does
        /// not name its face) and on the legacy callers; paint then walks
        /// `text` as before. A lane that edits the emitter keeps this field.
        run: Option<std::sync::Arc<GlyphRun>>,
    },
    /// Draw text decoration line (underline, strikethrough, overline).
""",
        1,
    ),
    (
        """                let mut advances = shape_line_advances(&text, style, font_size);
                // A justified line widens each word separator by the slack
""",
        """                let shaped = shape_line(&text, style, font_size);
                let mut advances = shaped.as_ref().and_then(char_advances_of);
                // SHAPED-RUN CONTRACT (S0): the same shape call, frozen with
                // the justification slack in it. `advances` above is its
                // projection onto characters and stays for the old path.
                let line_run = shaped
                    .as_ref()
                    .and_then(|shaped| GlyphRun::freeze(shaped, justify_space));
                // A justified line widens each word separator by the slack
""",
        1,
    ),
    (
        """                let (text, advances, text_width) = match (self.ellipsis.as_mut(), &advances) {
""",
        """                let (text, advances, text_width, line_run) = match (self.ellipsis.as_mut(), &advances) {
""",
        1,
    ),
    (
        """                            TextOverflowCut::Keep => (text, advances, text_width),
                            TextOverflowCut::Hide => continue,
                            TextOverflowCut::Cut {
                                text,
                                advances,
                                width,
                            } => (text, Some(advances), width),
                        }
                    }
                    _ => (text, advances, text_width),
                };
""",
        """                            TextOverflowCut::Keep => (text, advances, text_width, line_run),
                            TextOverflowCut::Hide => continue,
                            TextOverflowCut::Cut {
                                text,
                                advances,
                                width,
                            } => {
                                // The run is cut where the characters were:
                                // the kept glyphs, then the ellipsis shaped
                                // alone in the same face (as its advance was).
                                let kept = advances.len().saturating_sub(1);
                                let line_run = line_run.and_then(|run| {
                                    shape_line("\\u{2026}", style, font_size)
                                        .and_then(|tail| GlyphRun::freeze(&tail, 0.0))
                                        .and_then(|tail| run.cut_with_tail(kept, &tail))
                                });
                                (text, Some(advances), width, line_run)
                            }
                        }
                    }
                    _ => (text, advances, text_width, line_run),
                };
""",
        1,
    ),
    (
        """                    advances,
                    ascent: Some(seat_ascent),
                });

                // Draw text decorations
""",
        """                    advances,
                    ascent: Some(seat_ascent),
                    run: line_run.map(std::sync::Arc::new),
                });

                // Draw text decorations
""",
        1,
    ),
    (
        """    let shaper = TextShaper::new();
    let chain = FontFamilyChain::from_css_value(&style.font_family);
    let mut run = shaper
        .shape(
            text,
            &chain,
            style.font_weight,
            style.font_style,
            style.font_stretch,
            font_size,
        )
        .ok()?;
    run.apply_spacing(letter_spacing, word_spacing);
    if run.glyphs.len() != text.chars().count() {
        return None;
    }
    Some(run.glyphs.iter().map(|g| g.advance).collect())
}
""",
        """    let shaper = TextShaper::new();
    let chain = FontFamilyChain::from_css_value(&style.font_family);
    let mut run = shaper
        .shape(
            text,
            &chain,
            style.font_weight,
            style.font_style,
            style.font_stretch,
            font_size,
        )
        .ok()?;
    run.apply_spacing(letter_spacing, word_spacing);
    Some(run)
}

/// The per-character projection of a shaped line: `None` when a glyph is
/// not one character (the vector cannot describe it).
fn char_advances_of(run: &ShapedRun) -> Option<Vec<f32>> {
    if run.glyphs.len() != run.text.chars().count() {
        return None;
    }
    Some(run.glyphs.iter().map(|g| g.advance).collect())
}

/// Per-CHAR advances from the layout shaper, letter/word-spacing applied
/// (ADVANCE CONTRACT, text-stack unification 2026-07-11). Returns None when
/// shaping fails or when glyph count != char count (ligature clusters) — the
/// renderer then falls back to its own advances instead of misaligning.
pub fn shape_line_advances(
    text: &str,
    style: &rustkit_css::ComputedStyle,
    font_size: f32,
) -> Option<Vec<f32>> {
    shape_line(text, style, font_size)
        .as_ref()
        .and_then(char_advances_of)
}

/// The frozen run for one line of `text` in `style` (SHAPED-RUN CONTRACT,
/// slice S0): the shape `shape_line_advances` projects, kept whole.
/// `justify_space` is added to each word separator before the freeze.
/// `None` when shaping fails or the run is outside the slice
/// (`GlyphRun::freeze`).
pub fn shape_line_run(
    text: &str,
    style: &rustkit_css::ComputedStyle,
    font_size: f32,
    justify_space: f32,
) -> Option<GlyphRun> {
    shape_line(text, style, font_size)
        .as_ref()
        .and_then(|run| GlyphRun::freeze(run, justify_space))
}
""",
        1,
    ),
    (
        """/// Per-CHAR advances from the layout shaper, letter/word-spacing applied
/// (ADVANCE CONTRACT, text-stack unification 2026-07-11). Returns None when
/// shaping fails or when glyph count != char count (ligature clusters) — the
/// renderer then falls back to its own advances instead of misaligning.
pub fn shape_line_advances(
    text: &str,
    style: &rustkit_css::ComputedStyle,
    font_size: f32,
) -> Option<Vec<f32>> {
    let letter_spacing""",
        """/// One line of `text` shaped in `style`, letter/word-spacing applied: the
/// single shape call behind `shape_line_advances` and `shape_line_run`.
fn shape_line(
    text: &str,
    style: &rustkit_css::ComputedStyle,
    font_size: f32,
) -> Option<ShapedRun> {
    let letter_spacing""",
        1,
    ),
]
patch(root + "/lib.rs", lib)

legacy = (
    """            advances: None,
            ascent: None,
        });""",
    """            advances: None,
            ascent: None,
            run: None,
        });""",
)
for f in ("forms.rs", "images.rs"):
    s = open(root + "/" + f).read()
    print(f, "Text sites", s.count("DisplayCommand::Text {"), "legacy pattern", s.count(legacy[0]))
print("ok")
