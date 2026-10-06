//! A wheel over the content NSView must reach the app's window loop
//! (Z lane I0, hand-test item H6, 2026-10-05).
//!
//! hiwave-app scrolls the page from tao's `WindowEvent::MouseWheel`. The
//! content view has no `scrollWheel:` of its own, so that event only exists
//! if AppKit's responder chain carries the wheel from the content view up to
//! tao's view. This builds the same nesting (a tao window, the content view
//! made by `ViewHost::create_view` from the window's handle) and hands a
//! real scroll `NSEvent` to the view AppKit's own hit test picks.
//!
//! Runs without the test harness: tao's event loop must own the main
//! thread. The window is never shown, so this does not show AppKit itself
//! routing a wheel to the view under the pointer; it shows where the wheel
//! goes once the content view has it. No chrome WebViews are in the window.

#[cfg(not(target_os = "macos"))]
fn main() {}

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventCreateScrollWheelEvent(
        source: *const std::ffi::c_void,
        units: u32,
        wheel_count: u32,
        wheel1: i32,
        ...
    ) -> *mut std::ffi::c_void;
}

#[cfg(target_os = "macos")]
fn main() {
    use cocoa::base::{id, nil};
    use cocoa::foundation::NSPoint;
    use objc::{class, msg_send, sel, sel_impl};
    use raw_window_handle::HasWindowHandle;
    use rustkit_viewhost::{Bounds, ViewHost};
    use std::time::{Duration, Instant};
    use tao::dpi::LogicalSize;
    use tao::event::{Event, MouseScrollDelta, WindowEvent};
    use tao::event_loop::{ControlFlow, EventLoop};
    use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS, WindowExtMacOS};
    use tao::platform::run_return::EventLoopExtRunReturn;
    use tao::window::WindowBuilder;

    /// kCGScrollEventUnitPixel
    const UNIT_PIXEL: u32 = 0;
    /// One notch down, in pixels.
    const WHEEL_DOWN: i32 = -120;

    let screen_count: usize = unsafe {
        let screens: id = msg_send![class!(NSScreen), screens];
        if screens == nil {
            0
        } else {
            msg_send![screens, count]
        }
    };
    if screen_count == 0 {
        println!("skipped: no display in this session");
        return;
    }

    let mut event_loop = EventLoop::new();
    event_loop.set_activation_policy(ActivationPolicy::Accessory);
    let window = WindowBuilder::new()
        .with_inner_size(LogicalSize::new(1280.0, 800.0))
        .with_visible(false)
        .build(&event_loop)
        .expect("tao window");

    // Content below an 80px chrome strip, as hiwave-app lays it out.
    let host = ViewHost::new();
    let parent = window.window_handle().expect("window handle").as_raw();
    let _view = host
        .create_view(parent, Bounds::new(0, 80, 1280, 720))
        .expect("create_view");

    let tao_view = window.ns_view() as id;
    let class_name = |view: id| -> String {
        if view == nil {
            return "nil".to_string();
        }
        unsafe {
            let name: id = msg_send![view, className];
            let utf8: *const std::os::raw::c_char = msg_send![name, UTF8String];
            std::ffi::CStr::from_ptr(utf8)
                .to_string_lossy()
                .into_owned()
        }
    };

    let mut sent_at: Option<Instant> = None;
    let mut heard: Option<MouseScrollDelta> = None;
    let mut hit_class = String::new();
    event_loop.run_return(|event, _, control_flow| {
        *control_flow = ControlFlow::Poll;
        match event {
            Event::WindowEvent {
                event: WindowEvent::MouseWheel { delta, .. },
                ..
            } => {
                heard = Some(delta);
                *control_flow = ControlFlow::Exit;
            }
            Event::MainEventsCleared => match sent_at {
                None => unsafe {
                    // The middle of the content area, in the window's
                    // coordinates (origin bottom-left).
                    let frame_view: id = msg_send![tao_view, superview];
                    let hit: id = msg_send![frame_view, hitTest: NSPoint::new(640.0, 360.0)];
                    hit_class = class_name(hit);
                    let cg =
                        CGEventCreateScrollWheelEvent(std::ptr::null(), UNIT_PIXEL, 1, WHEEL_DOWN);
                    assert!(!cg.is_null(), "a scroll CGEvent was made");
                    let wheel: id = msg_send![class!(NSEvent), eventWithCGEvent: cg];
                    assert!(wheel != nil, "the CGEvent became an NSEvent");
                    if hit != nil {
                        let _: () = msg_send![hit, scrollWheel: wheel];
                    }
                    sent_at = Some(Instant::now());
                },
                Some(at) if at.elapsed() > Duration::from_secs(2) => {
                    *control_flow = ControlFlow::Exit;
                }
                Some(_) => {}
            },
            _ => {}
        }
    });

    assert_eq!(
        hit_class, "RustKitContentView",
        "the content view is under the point"
    );
    match heard {
        Some(delta) => {
            let dy = match delta {
                MouseScrollDelta::PixelDelta(p) => p.y,
                MouseScrollDelta::LineDelta(_, y) => f64::from(y),
                _ => 0.0,
            };
            assert!(
                dy < 0.0,
                "a wheel turned down scrolls down (negative y, the sign hiwave-app expects), got {delta:?}"
            );
            println!("ok: a wheel on the content view reached the window loop as {delta:?}");
        }
        None => panic!(
            "a wheel handed to the content view never became a tao MouseWheel event (waited 2 s)"
        ),
    }
}
