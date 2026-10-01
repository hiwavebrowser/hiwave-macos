"""Split GlyphRasterizer::rasterize_char into a char lookup plus
rasterize_glyph_id (the body after the lookup, moved verbatim).
usage: split_raster.py <macos.rs>"""
import sys

path = sys.argv[1]
src = open(path, "rb").read().decode("utf-8")
lines = src.splitlines(keepends=True)


def find(prefix, start=0):
    for i in range(start, len(lines)):
        if lines[i].strip().startswith(prefix):
            return i
    raise SystemExit("not found: " + prefix)


fn = find("pub fn rasterize_char(")
unsafe_i = find("unsafe {", fn)
c_void_i = find("use std::os::raw::c_void;", fn)
ext_open = find('extern "C" {', fn)
adv_decl = find("fn CTFontGetAdvancesForGlyphs(", ext_open)
ext_close = next(i for i in range(adv_decl, len(lines)) if lines[i].rstrip("\r\n") == "            }")
success_i = find("let success = CTFontGetGlyphsForCharacters(", ext_close)
fallback_i = find("return self.rasterize_fallback(ch);", success_i)
after_fallback = fallback_i + 2  # the closing brace of the `if`, then on
end_fn = find("/// Fallback rasterization for characters without glyphs", fn)
# end_fn - 1 is blank, end_fn - 2 closes the fn, end_fn - 3 closes unsafe
assert lines[end_fn - 2].rstrip("\r\n") == "    }", repr(lines[end_fn - 2])
assert c_void_i < ext_open < adv_decl < ext_close < success_i < fallback_i

moved_externs = lines[adv_decl:ext_close]
# drop the blank line that separated the first declaration from the rest
head = lines[:c_void_i] + lines[c_void_i + 1:adv_decl]
while head[-1].strip() == "":
    head.pop()
body = lines[after_fallback:end_fn - 1]

new_mid = [
    "            }\n",
    "\n",
] + lines[success_i:after_fallback] + [
    "\n",
    "            self.rasterize_glyph_id(glyphs[0], subpixel_x)\n",
    "        }\n",
    "    }\n",
    "\n",
    "    /// Rasterize glyph `glyph` of THIS face: no character lookup, and no\n",
    "    /// fallback face. A shaped run names its glyphs by id, so paint draws\n",
    "    /// exactly what layout measured. `rasterize_char` ends here once it has\n",
    "    /// found the character's glyph, so both entries return the same bitmap\n",
    "    /// for the same glyph. Same return tuple and contracts as\n",
    "    /// `rasterize_char`.\n",
    "    pub fn rasterize_glyph_id(\n",
    "        &self,\n",
    "        glyph: u16,\n",
    "        subpixel_x: f32,\n",
    "    ) -> Option<(Vec<u8>, u32, u32, f32, f32, f32)> {\n",
    "        let subpixel_x = if subpixel_x.is_finite() {\n",
    "            subpixel_x.clamp(0.0, 1.0 - f32::EPSILON)\n",
    "        } else {\n",
    "            0.0\n",
    "        };\n",
    "        let glyphs: [u16; 1] = [glyph];\n",
    "\n",
    "        unsafe {\n",
    "            use core_text::font::CTFontRef;\n",
    "            use std::os::raw::c_void;\n",
    "\n",
    "            let font_ref = self.font.as_concrete_TypeRef();\n",
    "\n",
    "            extern \"C\" {\n",
] + moved_externs + [
    "            }\n",
]

out = head + new_mid + body + lines[end_fn - 1:]
open(path, "wb").write("".join(out).encode("utf-8"))
print("ok", len(lines), "->", len(out))
