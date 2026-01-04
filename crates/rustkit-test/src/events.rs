//! # Event Handling Tests
//!
//! Tests for the DOM event system and input handling.

use rustkit_core::{
    FocusEvent, FocusEventType, InputEvent, KeyCode, KeyEvent, KeyEventType, KeyboardState,
    Modifiers, MouseButton, MouseEvent, MouseEventType, MouseState, Point,
};
use rustkit_css::ComputedStyle;
use rustkit_dom::events::{DomEvent, Event};
use rustkit_dom::FocusEventData;
use rustkit_layout::{BoxType, Dimensions, LayoutBox, Rect};

/// Test basic input event types.
#[test]
fn test_input_event_types() {
    // Mouse event
    let mouse = MouseEvent::new(MouseEventType::MouseDown, Point::new(100.0, 200.0))
        .with_button(MouseButton::Primary)
        .with_click_count(1);

    assert_eq!(mouse.event_type, MouseEventType::MouseDown);
    assert_eq!(mouse.position.x, 100.0);
    assert_eq!(mouse.position.y, 200.0);
    assert_eq!(mouse.button, MouseButton::Primary);
    assert_eq!(mouse.click_count, 1);

    // Key event
    let key = KeyEvent::new(KeyEventType::KeyDown, KeyCode::KeyA, Modifiers::new());
    assert_eq!(key.event_type, KeyEventType::KeyDown);
    assert_eq!(key.key_code, KeyCode::KeyA);
    assert_eq!(key.key, "a");
    assert_eq!(key.code, "KeyA");

    // Key with shift
    let key_shift = KeyEvent::new(
        KeyEventType::KeyDown,
        KeyCode::KeyA,
        Modifiers::new().with_shift(),
    );
    assert_eq!(key_shift.key, "A");

    // Focus event
    let focus = FocusEvent::new(FocusEventType::Focus);
    assert_eq!(focus.event_type, FocusEventType::Focus);
}

/// Test keyboard state tracking.
#[test]
fn test_keyboard_state() {
    let mut state = KeyboardState::new();

    // Press 'A' - first press is not repeat
    assert!(!state.key_down(KeyCode::KeyA));
    assert!(state.is_pressed(KeyCode::KeyA));

    // Press 'A' again - this is a repeat
    assert!(state.key_down(KeyCode::KeyA));

    // Release 'A'
    state.key_up(KeyCode::KeyA);
    assert!(!state.is_pressed(KeyCode::KeyA));

    // Press Shift - should update modifiers
    state.key_down(KeyCode::ShiftLeft);
    assert!(state.modifiers().shift);

    // Press Ctrl
    state.key_down(KeyCode::ControlLeft);
    assert!(state.modifiers().ctrl);
    assert!(state.modifiers().shift);

    // Release Shift
    state.key_up(KeyCode::ShiftLeft);
    assert!(!state.modifiers().shift);
    assert!(state.modifiers().ctrl);
}

/// Test mouse state tracking.
#[test]
fn test_mouse_state() {
    let mut state = MouseState::new();

    // Update position
    state.set_position(Point::new(100.0, 200.0));
    assert_eq!(state.position.x, 100.0);
    assert_eq!(state.position.y, 200.0);

    // Press primary button
    state.button_down(MouseButton::Primary);
    assert!(state.is_pressed(MouseButton::Primary));
    assert_eq!(state.buttons, 1);

    // Press secondary button
    state.button_down(MouseButton::Secondary);
    assert!(state.is_pressed(MouseButton::Secondary));
    assert_eq!(state.buttons, 3); // 1 + 2

    // Release primary
    state.button_up(MouseButton::Primary);
    assert!(!state.is_pressed(MouseButton::Primary));
    assert!(state.is_pressed(MouseButton::Secondary));
    assert_eq!(state.buttons, 2);
}

/// Test DOM event creation.
#[test]
fn test_dom_event_creation() {
    // Click event
    let click = Event::new("click", true, true);
    assert_eq!(click.event_type, "click");
    assert!(click.bubbles);
    assert!(click.cancelable);
    assert!(!click.is_trusted);

    // Trusted event
    let trusted = Event::new_trusted("mousedown", true, true);
    assert!(trusted.is_trusted);

    // Focus event (doesn't bubble)
    let focus = DomEvent::focus("focus", FocusEventData::default());
    assert!(!focus.event().bubbles);

    // Focusin event (bubbles)
    let focusin = DomEvent::focus("focusin", FocusEventData::default());
    assert!(focusin.event().bubbles);
}

/// Test event propagation control.
#[test]
fn test_event_propagation() {
    let event = Event::new("click", true, true);

    // Initially not stopped
    assert!(!event.propagation_stopped());
    assert!(!event.immediate_propagation_stopped());

    // Stop propagation
    event.stop_propagation();
    assert!(event.propagation_stopped());
    assert!(!event.immediate_propagation_stopped());

    // New event for immediate stop
    let event2 = Event::new("click", true, true);
    event2.stop_immediate_propagation();
    assert!(event2.propagation_stopped());
    assert!(event2.immediate_propagation_stopped());
}

/// Test prevent default.
#[test]
fn test_prevent_default() {
    // Cancelable event
    let cancelable = Event::new("click", true, true);
    assert!(!cancelable.default_prevented());
    cancelable.prevent_default();
    assert!(cancelable.default_prevented());

    // Non-cancelable event
    let non_cancelable = Event::new("load", false, false);
    non_cancelable.prevent_default();
    assert!(!non_cancelable.default_prevented());
}

/// Test key code conversion.
#[test]
fn test_key_code_conversion() {
    // Letter keys
    assert_eq!(KeyCode::from_vk(0x41), KeyCode::KeyA);
    assert_eq!(KeyCode::from_vk(0x5A), KeyCode::KeyZ);

    // Numbers
    assert_eq!(KeyCode::from_vk(0x30), KeyCode::Digit0);
    assert_eq!(KeyCode::from_vk(0x39), KeyCode::Digit9);

    // Function keys
    assert_eq!(KeyCode::from_vk(0x70), KeyCode::F1);
    assert_eq!(KeyCode::from_vk(0x7B), KeyCode::F12);

    // Special keys
    assert_eq!(KeyCode::from_vk(0x1B), KeyCode::Escape);
    assert_eq!(KeyCode::from_vk(0x0D), KeyCode::Enter);
    assert_eq!(KeyCode::from_vk(0x09), KeyCode::Tab);
    assert_eq!(KeyCode::from_vk(0x08), KeyCode::Backspace);
    assert_eq!(KeyCode::from_vk(0x20), KeyCode::Space);

    // Arrow keys
    assert_eq!(KeyCode::from_vk(0x25), KeyCode::ArrowLeft);
    assert_eq!(KeyCode::from_vk(0x26), KeyCode::ArrowUp);
    assert_eq!(KeyCode::from_vk(0x27), KeyCode::ArrowRight);
    assert_eq!(KeyCode::from_vk(0x28), KeyCode::ArrowDown);

    // Unknown key
    assert_eq!(KeyCode::from_vk(0xFF), KeyCode::Unknown);
}

/// Test hit testing.
#[test]
fn test_hit_testing_basic() {
    let style = ComputedStyle::new();
    let mut root = LayoutBox::new(BoxType::Block, style.clone());

    // Set up dimensions for root
    root.dimensions = Dimensions {
        content: Rect::new(0.0, 0.0, 800.0, 600.0),
        ..Default::default()
    };

    // Hit inside
    let result = root.hit_test(100.0, 100.0);
    assert!(result.is_some());
    let hit = result.unwrap();
    assert_eq!(hit.local_x, 100.0);
    assert_eq!(hit.local_y, 100.0);
    assert_eq!(hit.depth, 0);

    // Hit outside
    let result = root.hit_test(900.0, 700.0);
    assert!(result.is_none());
}

/// Test hit testing with children.
#[test]
fn test_hit_testing_children() {
    let style = ComputedStyle::new();
    let mut root = LayoutBox::new(BoxType::Block, style.clone());

    root.dimensions = Dimensions {
        content: Rect::new(0.0, 0.0, 800.0, 600.0),
        ..Default::default()
    };

    // Add a child
    let mut child = LayoutBox::new(BoxType::Block, style.clone());
    child.dimensions = Dimensions {
        content: Rect::new(100.0, 100.0, 200.0, 200.0),
        ..Default::default()
    };
    root.children.push(child);

    // Hit inside child
    let result = root.hit_test(150.0, 150.0);
    assert!(result.is_some());
    let hit = result.unwrap();
    assert_eq!(hit.depth, 1); // Child is at depth 1

    // Hit outside child but inside root
    let result = root.hit_test(50.0, 50.0);
    assert!(result.is_some());
    let hit = result.unwrap();
    assert_eq!(hit.depth, 0); // Root is at depth 0
}

/// Test mouse button masks.
#[test]
fn test_mouse_button_masks() {
    assert_eq!(MouseButton::Primary.button_mask(), 1);
    assert_eq!(MouseButton::Secondary.button_mask(), 2);
    assert_eq!(MouseButton::Auxiliary.button_mask(), 4);
    assert_eq!(MouseButton::Back.button_mask(), 8);
    assert_eq!(MouseButton::Forward.button_mask(), 16);
}

/// Test modifier key combinations.
#[test]
fn test_modifier_combinations() {
    let mods = Modifiers::new().with_ctrl().with_shift();
    assert!(mods.ctrl);
    assert!(mods.shift);
    assert!(!mods.alt);
    assert!(!mods.meta);
    assert!(mods.any());

    let empty = Modifiers::new();
    assert!(empty.none());
    assert!(!empty.any());
}

/// Test input event wrapper.
#[test]
fn test_input_event_wrapper() {
    let mouse = MouseEvent::new(MouseEventType::MouseMove, Point::new(10.0, 20.0));
    let input = InputEvent::Mouse(mouse);
    assert!(matches!(input, InputEvent::Mouse(_)));

    let key = KeyEvent::new(KeyEventType::KeyDown, KeyCode::Enter, Modifiers::new());
    let input = InputEvent::Key(key);
    assert!(matches!(input, InputEvent::Key(_)));

    let focus = FocusEvent::new(FocusEventType::Focus);
    let input = InputEvent::Focus(focus);
    assert!(matches!(input, InputEvent::Focus(_)));
}
