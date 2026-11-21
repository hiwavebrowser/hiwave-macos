//! FORM-CONTROL-FLOAT-AVOID: block-level form controls and `fieldset` avoid
//! floats (Chrome 148 lays them out as their own formatting context). The
//! headless engine oracle `form_control_float_avoid_tests` covers layout
//! geometry; open #1389 pins `place_beside_floats` margins for replaced/BFC
//! only. These pins assert the `avoids_floats` predicate on default `--lib`.

use super::*;
use rustkit_css::Display;

fn identity(tag: &str) -> ElementIdentity {
    ElementIdentity {
        element_id: 1,
        tag: tag.to_string(),
        selector: tag.to_string(),
    }
}

fn text_input() -> FormControlType {
    FormControlType::TextInput {
        value: String::new(),
        placeholder: String::new(),
        input_type: "text".to_string(),
        size: 20,
    }
}

fn form_control(display: Display) -> LayoutBox {
    let mut s = ComputedStyle::new();
    s.display = display;
    LayoutBox::new(BoxType::FormControl(text_input()), s)
}

fn block_with_tag(tag: &str) -> LayoutBox {
    let mut b = LayoutBox::new(BoxType::Block, ComputedStyle::new());
    b.set_identity(identity(tag));
    b
}

#[test]
fn a_block_form_control_avoids_floats_and_an_inline_level_one_does_not() {
    // Block-level only: inline and atomic-inline (inline-block) stay in the
    // line and do not take the float-avoid path.
    assert!(LayoutBox::avoids_floats(&form_control(Display::Block)));
    assert!(!LayoutBox::avoids_floats(&form_control(Display::Inline)));
    assert!(!LayoutBox::avoids_floats(&form_control(Display::InlineBlock)));
}

#[test]
fn a_fieldset_block_avoids_floats_and_a_plain_block_does_not() {
    assert!(LayoutBox::avoids_floats(&block_with_tag("fieldset")));
    assert!(LayoutBox::avoids_floats(&block_with_tag("input")));
    assert!(LayoutBox::avoids_floats(&block_with_tag("select")));
    assert!(LayoutBox::avoids_floats(&block_with_tag("textarea")));
    assert!(LayoutBox::avoids_floats(&block_with_tag("button")));
    assert!(!LayoutBox::avoids_floats(&block_with_tag("div")));
    assert!(!LayoutBox::avoids_floats(&LayoutBox::new(
        BoxType::Block,
        ComputedStyle::new()
    )));
}

#[test]
fn a_floated_or_out_of_flow_box_never_avoids_floats() {
    let mut floated = form_control(Display::Block);
    floated.float = Float::Left;
    assert!(!LayoutBox::avoids_floats(&floated));

    let mut abs = form_control(Display::Block);
    abs.position = Position::Absolute;
    assert!(!LayoutBox::avoids_floats(&abs));
}
