"""Add FaceIdentity / GlyphRun to rustkit-layout text.rs and set the face on
the macOS shaped run. usage: patch_text.py <text.rs> <types.rs.txt>"""
import sys

p = sys.argv[1]
types = open(sys.argv[2]).read()
s = open(p).read()

old = """        // Fallback to system font if nothing found
        let ct_font = ct_font_opt.unwrap_or_else(|| {
            ct_font::new_from_name("Helvetica", size as f64).unwrap_or_else(|_| {
                ct_font::new_from_name(".AppleSystemUIFont", size as f64).unwrap()
            })
        });
"""
new = """        // Fallback to system font if nothing found
        let resolved_from_chain = ct_font_opt.is_some();
        let ct_font = ct_font_opt.unwrap_or_else(|| {
            ct_font::new_from_name("Helvetica", size as f64).unwrap_or_else(|_| {
                ct_font::new_from_name(".AppleSystemUIFont", size as f64).unwrap()
            })
        });

        // Name the face the glyph ids below belong to, and keep the font, so
        // paint can draw this run with it instead of resolving the family
        // list a second time (see `FaceIdentity`).
        let italic = style == FontStyle::Italic;
        let web_face = if resolved_from_chain {
            rustkit_text::webfonts::face_id(&used_family, weight.0, italic)
        } else {
            0
        };
        let (face_id, postscript_name) =
            rustkit_text::macos::intern_face(&ct_font, size, web_face, weight.0, italic);
        let face = FaceIdentity {
            id: face_id,
            postscript_name,
            face_index: 0,
        };
"""
assert s.count(old) == 1
s = s.replace(old, new)

old2 = """                metrics,
                direction: TextDirection::Ltr,
                face: None,
            })
        }
    }

    /// Per-UTF-16-unit kerning adjustments"""
new2 = old2.replace("face: None,", "face: Some(face),")
assert s.count(old2) == 1
s = s.replace(old2, new2)

old3 = """impl ShapedRun {
    /// Get the total width of the run.
"""
assert s.count(old3) == 1
s = s.replace(old3, types + "\n" + old3)
open(p, "w").write(s)
print("ok")
