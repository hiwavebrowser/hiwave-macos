//! CH2.E3: the content view is an `NSTextInputClient` and answers the Edit
//! menu's actions.
//!
//! macOS hands a dead key's or an IME's text to the first responder through
//! `NSTextInputClient` (`setMarkedText:selectedRange:replacementRange:`,
//! `insertText:replacementRange:`, `unmarkText`, ...), and the Edit menu's
//! predefined items send `copy:`, `cut:`, `paste:`, `undo:`, `redo:` and
//! `selectAll:` down the responder chain. The view answered none of them:
//! dead keys typed nothing composed, and the Edit menu did nothing.
//!
//! The calls are made on the view directly, as the input context and the
//! menu make them. Runs without the test harness: AppKit only makes windows
//! on the main thread. The window is never shown.

#[cfg(not(target_os = "macos"))]
fn main() {}

#[cfg(target_os = "macos")]
fn main() {
    use cocoa::appkit::{
        NSApp, NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSWindow,
        NSWindowStyleMask,
    };
    use cocoa::base::{id, nil, BOOL, NO, YES};
    use cocoa::foundation::{NSAutoreleasePool, NSPoint, NSRange, NSRect, NSSize, NSString};
    use objc::runtime::{Protocol, Sel};
    use objc::{class, msg_send, sel, sel_impl};
    use raw_window_handle::{AppKitWindowHandle, RawWindowHandle};
    use rustkit_viewhost::{
        drain_input, set_text_input_state, Bounds, EditAction, ImeInput, InputKind, TextInputMirror,
        ViewHost,
    };
    use std::ptr::NonNull;

    const NOT_FOUND: u64 = i64::MAX as u64; // NSNotFound

    unsafe {
        let _pool = NSAutoreleasePool::new(nil);
        let screens: id = msg_send![class!(NSScreen), screens];
        let screen_count: usize = if screens == nil { 0 } else { msg_send![screens, count] };
        if screen_count == 0 {
            println!("skipped: no display in this session");
            return;
        }
        let app = NSApp();
        app.setActivationPolicy_(NSApplicationActivationPolicy::NSApplicationActivationPolicyAccessory);
        let window = NSWindow::alloc(nil).initWithContentRect_styleMask_backing_defer_(
            NSRect::new(NSPoint::new(100.0, 100.0), NSSize::new(1280.0, 800.0)),
            NSWindowStyleMask::NSTitledWindowMask | NSWindowStyleMask::NSResizableWindowMask,
            NSBackingStoreType::NSBackingStoreBuffered,
            NO,
        );
        let content_view: id = msg_send![window, contentView];
        let parent = RawWindowHandle::AppKit(AppKitWindowHandle::new(
            NonNull::new(content_view as *mut std::ffi::c_void).expect("content view"),
        ));
        let host = ViewHost::new();
        let view_id = host.create_view(parent, Bounds::new(0, 80, 1280, 720)).expect("create_view");
        let frame_view: id = msg_send![content_view, superview];
        let page: id = msg_send![frame_view, hitTest: NSPoint::new(10.0, 800.0 - 95.0)];
        assert!(page != nil, "the content view is under the point");

        let responds = |sel: Sel| -> bool {
            let r: BOOL = msg_send![page, respondsToSelector: sel];
            r == YES
        };
        let protocol = Protocol::get("NSTextInputClient").expect("NSTextInputClient protocol");
        let conforms: BOOL = msg_send![page, conformsToProtocol: protocol];
        let mut missing: Vec<&str> = Vec::new();
        if conforms != YES {
            missing.push("conformsToProtocol:NSTextInputClient");
        }
        for (name, sel) in [
            ("setMarkedText:selectedRange:replacementRange:", sel!(setMarkedText:selectedRange:replacementRange:)),
            ("insertText:replacementRange:", sel!(insertText:replacementRange:)),
            ("unmarkText", sel!(unmarkText)),
            ("hasMarkedText", sel!(hasMarkedText)),
            ("markedRange", sel!(markedRange)),
            ("selectedRange", sel!(selectedRange)),
            ("firstRectForCharacterRange:actualRange:", sel!(firstRectForCharacterRange:actualRange:)),
            ("validAttributesForMarkedText", sel!(validAttributesForMarkedText)),
            ("attributedSubstringForProposedRange:actualRange:", sel!(attributedSubstringForProposedRange:actualRange:)),
            ("characterIndexForPoint:", sel!(characterIndexForPoint:)),
            ("doCommandBySelector:", sel!(doCommandBySelector:)),
            ("copy:", sel!(copy:)),
            ("cut:", sel!(cut:)),
            ("paste:", sel!(paste:)),
            ("undo:", sel!(undo:)),
            ("redo:", sel!(redo:)),
            ("selectAll:", sel!(selectAll:)),
            ("performKeyEquivalent:", sel!(performKeyEquivalent:)),
        ] {
            // performKeyEquivalent: is NSView's; it must be the view's own.
            let own = if name == "performKeyEquivalent:" {
                let ours: *const std::ffi::c_void = msg_send![page, methodForSelector: sel];
                let views: *const std::ffi::c_void = msg_send![class!(NSView), instanceMethodForSelector: sel];
                ours != views
            } else {
                responds(sel)
            };
            if !own {
                missing.push(name);
            }
        }
        assert!(missing.is_empty(), "the content view does not answer: {missing:?}");

        let take = || -> Vec<InputKind> { drain_input().into_iter().map(|e| e.kind).collect() };
        take();
        let string = |s: &str| -> id { NSString::alloc(nil).init_str(s) };
        let none = NSRange::new(NOT_FOUND, 0);

        // The page's last report: caret at 2 of "ab", painted at 40,10.
        set_text_input_state(
            view_id,
            Some(TextInputMirror { marked: None, selected: (2, 2), caret: Some((40.0, 10.0, 20.0)) }),
        );
        let sel: NSRange = msg_send![page, selectedRange];
        assert_eq!((sel.location, sel.length), (2, 0), "selectedRange is the page's selection");

        // A dead key: Option+e marks "´", e commits "é".
        let _: () = msg_send![page, setMarkedText: string("\u{b4}") selectedRange: NSRange::new(1, 0) replacementRange: none];
        let marked: BOOL = msg_send![page, hasMarkedText];
        assert_eq!(marked, YES, "text is marked");
        let range: NSRange = msg_send![page, markedRange];
        assert_eq!((range.location, range.length), (2, 1), "marked from the caret, one unit");
        let _: () = msg_send![page, insertText: string("\u{e9}") replacementRange: none];
        let marked: BOOL = msg_send![page, hasMarkedText];
        assert_eq!(marked, NO, "the commit ends the composition");
        let range: NSRange = msg_send![page, markedRange];
        assert_eq!(range.location, NOT_FOUND, "nothing marked");
        assert_eq!(
            take(),
            vec![
                InputKind::Ime(ImeInput::SetMarked("\u{b4}".into())),
                InputKind::Ime(ImeInput::Insert { text: "\u{e9}".into(), replace: None }),
            ]
        );

        // An attributed string (what an IME passes) and unmarkText.
        let attributed: id = msg_send![class!(NSAttributedString), alloc];
        let attributed: id = msg_send![attributed, initWithString: string("ni")];
        let _: () = msg_send![page, setMarkedText: attributed selectedRange: NSRange::new(2, 0) replacementRange: none];
        let _: () = msg_send![page, unmarkText];
        let marked: BOOL = msg_send![page, hasMarkedText];
        assert_eq!(marked, NO);
        assert_eq!(
            take(),
            vec![InputKind::Ime(ImeInput::SetMarked("ni".into())), InputKind::Ime(ImeInput::Unmark)]
        );

        // The press-and-hold accent popover: replace the letter before the caret.
        let _: () = msg_send![page, insertText: string("\u{e8}") replacementRange: NSRange::new(1, 1)];
        assert_eq!(take(), vec![InputKind::Ime(ImeInput::Insert { text: "\u{e8}".into(), replace: Some((1, 1)) })]);

        // The candidate window sits under the caret: a screen rect at the
        // caret's view-local 40,10, 20 high (the view's top is window y 80).
        let mut actual = NSRange::new(0, 0);
        let rect: NSRect = msg_send![page, firstRectForCharacterRange: NSRange::new(2, 0) actualRange: &mut actual];
        let window_frame: NSRect = msg_send![window, frame];
        let content_rect: NSRect = msg_send![window, contentRectForFrameRect: window_frame];
        assert_eq!(rect.origin.x, content_rect.origin.x + 40.0, "rect {} {} {} {}", rect.origin.x, rect.origin.y, rect.size.width, rect.size.height);
        assert_eq!(rect.size.height, 20.0);
        // Screen y is bottom-up: the caret's bottom is 80 + 10 + 20 below the content top.
        assert_eq!(rect.origin.y, content_rect.origin.y + content_rect.size.height - 110.0, "rect {} {} {} {}", rect.origin.x, rect.origin.y, rect.size.width, rect.size.height);

        // The Edit menu's actions.
        for sel in [sel!(copy:), sel!(cut:), sel!(paste:), sel!(undo:), sel!(redo:), sel!(selectAll:)] {
            let _: () = msg_send![page, performSelector: sel withObject: nil];
        }
        assert_eq!(
            take(),
            [EditAction::Copy, EditAction::Cut, EditAction::Paste, EditAction::Undo, EditAction::Redo, EditAction::SelectAll]
                .into_iter()
                .map(InputKind::Edit)
                .collect::<Vec<_>>()
        );

        // Chromium's reserved chords go to the menu, never the page
        // (Chrome 143 on this seat: no keydown for Cmd+T or Cmd+N, and the
        // tab opens though the page cancels; Cmd+L, Cmd+D, Cmd+K reach it).
        use rustkit_viewhost::is_reserved_chord;
        for (code, shift, alt, reserved, what) in [
            (17, false, false, true, "Cmd+T"),
            (45, false, false, true, "Cmd+N"),
            (13, false, false, true, "Cmd+W"),
            (12, false, false, true, "Cmd+Q"),
            (17, true, false, true, "Cmd+Shift+T"),
            (33, true, false, true, "Cmd+Shift+["),
            (124, false, true, true, "Cmd+Option+Right"),
            (37, false, false, false, "Cmd+L"),
            (2, false, false, false, "Cmd+D"),
            (40, false, false, false, "Cmd+K"),
            (3, false, false, false, "Cmd+F"),
            (9, false, false, false, "Cmd+V"),
            (33, false, false, false, "Cmd+[ (Back)"),
            (124, false, false, false, "Cmd+Right"),
        ] {
            assert_eq!(is_reserved_chord(code, true, shift, alt), reserved, "{what}");
        }
        assert!(!is_reserved_chord(17, false, false, false), "T without Cmd");

        println!("ok: the content view is a text input client and answers the Edit menu");
    }
}
