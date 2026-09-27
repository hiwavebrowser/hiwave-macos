#!/usr/bin/env python3
"""One-off edit for atlas/rs-inflow-pct-height: carry html's height on the layout root."""
import sys

repo = sys.argv[1]
p = f"{repo}/crates/rustkit-layout/src/lib.rs"
s = open(p).read()
old = """    /// The content height this box's children resolve percentage heights
    /// against, when the caller knows it and the box's own style does not
    /// say. The engine's layout root is an anonymous stand-in for `<html>`
    /// (html's computed style only feeds inheritance), so it sets this to
    /// html's definite height against the initial containing block:
    /// `html, body { height: 100% }` keeps body at the viewport's height.
    pub percent_base_for_children: Option<f32>,"""
new = """    /// Set on the engine's layout root, an anonymous stand-in for `<html>`
    /// whose own style is the default (html's computed style only feeds
    /// inheritance): html's specified `height`. The root's children resolve
    /// percentage heights against it, taken against the initial containing
    /// block (the viewport), so `html, body { height: 100% }` keeps body at
    /// the viewport's height while an `auto` html makes body's `100%` auto.
    pub root_element_height: Option<Length>,"""
assert s.count(old) == 1
s = s.replace(old, new)
s = s.replace("            percent_base_for_children: None,\n", "            root_element_height: None,\n", 1)
old = """        if self.percent_base_for_children.is_some() {
            return self.percent_base_for_children;
        }
"""
new = """        if let Some(html_height) = &self.root_element_height {
            let icb_height = self.viewport.1;
            return match html_height {
                Length::Auto => None,
                Length::Percent(pct) => Some(pct / 100.0 * icb_height),
                other => Some(self.length_to_px(other, icb_height)),
            }
            .map(|h| h.max(0.0));
        }
"""
assert s.count(old) == 1
s = s.replace(old, new)

i = s.find("fn html_and_body_at_100_percent_still_fill_the_viewport")
j = s.find("\n    }\n", i) + 7
test = '''fn html_and_body_at_100_percent_still_fill_the_viewport() {
        // The engine's root: anonymous, default style, html's height carried
        // in `root_element_height`, laid out with a cursor-height (0)
        // containing block.
        for (html_height, body_expected) in [
            (Length::Percent(100.0), 800.0),
            (Length::Px(300.0), 300.0),
            (Length::Auto, 0.0),
        ] {
            let mut body_style = ComputedStyle::new();
            body_style.height = Length::Percent(100.0);
            let mut root = LayoutBox::new(BoxType::Block, ComputedStyle::new());
            root.root_element_height = Some(html_height.clone());
            root.children.push(LayoutBox::new(BoxType::Block, body_style));
            root.set_viewport(1280.0, 800.0);
            let cb = Dimensions {
                content: Rect::new(0.0, 0.0, 1280.0, 0.0),
                ..Default::default()
            };
            let mut mc = MarginCollapseContext::new();
            mc.children_are_formatting_roots = true;
            let mut fc = FloatContext::new();
            root.layout_with_collapse(&cb, &mut mc, &mut fc);
            assert_eq!(
                root.children[0].dimensions.content.height, body_expected,
                "html {{ height: {html_height:?} }}"
            );
        }
    }
'''
s = s[:i] + test + s[j:]
open(p, "w").write(s)

p = f"{repo}/crates/rustkit-engine/src/lib.rs"
s = open(p).read()
old = """            root_box.children.push(body_box);
        } else if let Some(html) = document.document_element() {"""
new = """            root_box.children.push(body_box);
            // Percentage heights on body resolve against html's height
            // (CSS 2.1 §10.5), which the anonymous root stands in for.
            root_box.root_element_height = Some(
                html_style
                    .as_ref()
                    .map(|s| s.height.clone())
                    .unwrap_or(rustkit_css::Length::Auto),
            );
        } else if let Some(html) = document.document_element() {"""
assert s.count(old) == 1
s = s.replace(old, new)
open(p, "w").write(s)
print("ok")
