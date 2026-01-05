//! HiWave Smoke Harness - RustKit edition (macOS)
//!
//! This harness exercises the RustKit engine with scripted layout stress
//! tests to validate rendering stability. It uses WRY for chrome/shelf
//! and RustKit for the main content area.

use raw_window_handle::HasWindowHandle;
use rustkit_engine::EngineBuilder;
use rustkit_viewhost::Bounds;
use serde_json::json;
use std::time::{Duration, Instant};
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::window::WindowBuilder;
use tracing::{error, info};
use wry::dpi::{LogicalPosition, LogicalSize};
use wry::{Rect, WebViewBuilder};

#[derive(Debug, Clone)]
enum UserEvent {
    Layout {
        left: f64,
        right_open: bool,
        shelf: f64,
    },
    Exit,
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect {
        position: LogicalPosition::new(x, y).into(),
        size: LogicalSize::new(w.max(0.0), h.max(0.0)).into(),
    }
}

/// Parse command line arguments
struct Args {
    duration_ms: u64,
    dump_frame: Option<String>,
    html_file: Option<String>,
    width: u32,
    height: u32,
}

impl Args {
    fn parse() -> Self {
        let mut args = std::env::args().skip(1).peekable();
        let mut duration_ms = 4000u64;
        let mut dump_frame = None;
        let mut html_file = None;
        let mut width = 1100u32;
        let mut height = 640u32;

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--duration-ms" => {
                    if let Some(val) = args.next() {
                        duration_ms = val.parse().unwrap_or(4000);
                    }
                }
                "--dump-frame" => {
                    dump_frame = args.next();
                }
                "--html-file" => {
                    html_file = args.next();
                }
                "--width" => {
                    if let Some(val) = args.next() {
                        width = val.parse().unwrap_or(1100);
                    }
                }
                "--height" => {
                    if let Some(val) = args.next() {
                        height = val.parse().unwrap_or(640);
                    }
                }
                _ => {}
            }
        }

        Self {
            duration_ms,
            dump_frame,
            html_file,
            width,
            height,
        }
    }
    
    /// Load HTML content from file or use default test HTML
    fn load_html_content(&self) -> String {
        if let Some(ref path) = self.html_file {
            match std::fs::read_to_string(path) {
                Ok(content) => return content,
                Err(e) => {
                    eprintln!("Warning: Failed to read HTML file {}: {}", path, e);
                }
            }
        }
        
        // Default test HTML
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>RustKit Smoke Test</title>
    <style>
        :root {
            --bg-primary: #0b1a2a;
            --text-primary: #d7e8ff;
            --accent: #4a90d9;
        }
        body {
            margin: 0;
            padding: 20px;
            background: var(--bg-primary);
            color: var(--text-primary);
            font-family: system-ui, sans-serif;
        }
        .hero {
            text-align: center;
            padding: 40px 20px;
        }
        h1 {
            font-size: 32px;
            color: var(--accent);
            margin-bottom: 16px;
        }
        p {
            font-size: 18px;
            line-height: 1.6;
        }
        .box {
            background: rgba(74, 144, 217, 0.2);
            border: 1px solid var(--accent);
            border-radius: 8px;
            padding: 20px;
            margin: 20px auto;
            max-width: 600px;
        }
    </style>
</head>
<body>
    <div class="hero">
        <h1>RustKit Engine</h1>
        <p>This content is rendered by the RustKit browser engine.</p>
    </div>
    <div class="box">
        <p>If you can read this, the engine is working!</p>
        <p>Smoke test timestamp: <span id="time">loading...</span></p>
    </div>
</body>
</html>"#.to_string()
    }
}

fn spawn_scripted_flow(proxy: EventLoopProxy<UserEvent>, duration_ms: u64) {
    std::thread::spawn(move || {
        let start = Instant::now();

        // Phase 1: sidebar drag simulation
        for i in 0..30 {
            let left = (i as f64) * 8.0; // 0..240
            let right_open = i % 10 >= 5;
            let shelf = if i % 2 == 0 { 0.0 } else { 120.0 };
            let _ = proxy.send_event(UserEvent::Layout {
                left,
                right_open,
                shelf,
            });
            std::thread::sleep(Duration::from_millis(30));
        }

        // Phase 2: let engine render content
        // (we avoid network dependency in the harness itself)

        // Let UI settle
        while start.elapsed() < Duration::from_millis(duration_ms) {
            std::thread::sleep(Duration::from_millis(50));
        }

        let _ = proxy.send_event(UserEvent::Exit);
    });
}

fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let args = Args::parse();
    info!(
        duration_ms = args.duration_ms,
        dump_frame = ?args.dump_frame,
        html_file = ?args.html_file,
        width = args.width,
        height = args.height,
        "Starting HiWave Smoke Harness (RustKit)"
    );

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let window = WindowBuilder::new()
        .with_title("HiWave Smoke Harness (RustKit)")
        .with_inner_size(tao::dpi::LogicalSize::new(1100.0, 760.0))
        .build(&event_loop)
        .expect("Failed to create window");

    // Chrome bar (using WRY for simple UI)
    let chrome = WebViewBuilder::new()
        .with_html(
            r#"<!doctype html><meta charset='utf-8'/>
            <body style='margin:0;background:#111;color:#fff;font:16px system-ui;display:flex;align-items:center;justify-content:center;'>
              chrome
            </body>"#,
        )
        .with_bounds(rect(0.0, 0.0, 1100.0, 72.0))
        .build_as_child(&window)
        .expect("Failed to create chrome webview");

    // Shelf (using WRY for simple UI)
    let shelf = WebViewBuilder::new()
        .with_html(
            r#"<!doctype html><meta charset='utf-8'/>
            <body style='margin:0;background:#1a0b2a;color:#f0d7ff;font:16px system-ui;display:flex;align-items:center;justify-content:center;'>
              shelf
            </body>"#,
        )
        .with_bounds(rect(0.0, 760.0, 1100.0, 0.0))
        .build_as_child(&window)
        .expect("Failed to create shelf webview");

    // Content area (using RustKit engine)
    let mut engine = EngineBuilder::new()
        .build()
        .expect("Failed to create RustKit engine");

    // Get the raw window handle for creating the RustKit view
    let window_handle = window
        .window_handle()
        .expect("Failed to get window handle")
        .as_raw();

    // Use standardized content bounds from args for deterministic capture
    let chrome_height = 72u32;
    let content_bounds = Bounds {
        x: 0,
        y: chrome_height as i32,
        width: args.width,
        height: args.height.saturating_sub(chrome_height),
    };

    let content_view_id = engine
        .create_view(window_handle, content_bounds)
        .expect("Failed to create RustKit content view");

    // Load test content into the RustKit view (from file or default)
    let test_html = args.load_html_content();

    if let Err(e) = engine.load_html(content_view_id, &test_html) {
        error!(?e, "Failed to load HTML into RustKit view");
    }

    // Initial render
    if let Err(e) = engine.render_view(content_view_id) {
        error!(?e, "Failed to render RustKit view");
    }

    spawn_scripted_flow(proxy, args.duration_ms);

    let mut last_layout = (0.0_f64, false, 0.0_f64);
    let start = Instant::now();
    let dump_frame_path = args.dump_frame.clone();
    let mut frame_dumped = false;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                let result = json!({
                    "status": "fail",
                    "reason": "user_closed_window",
                    "elapsed_ms": start.elapsed().as_millis()
                });
                println!("{}", result);
                *control_flow = ControlFlow::Exit;
            }
            Event::WindowEvent {
                event: WindowEvent::Resized(size),
                ..
            } => {
                // Update RustKit view bounds on window resize
                let (left, right_open, shelf_h) = last_layout;
                let width = size.width as i32;
                let height = size.height as i32;

                let chrome_h = 72;
                let right_sidebar_width = if right_open { 220 } else { 0 };
                let left_i32 = left as i32;

                let content_w = (width - left_i32 - right_sidebar_width).max(0);
                let content_h = (height - chrome_h - shelf_h as i32).max(0);

                let bounds = Bounds {
                    x: left_i32,
                    y: chrome_h,
                    width: content_w as u32,
                    height: content_h as u32,
                };

                if let Err(e) = engine.resize_view(content_view_id, bounds) {
                    error!(?e, "Failed to resize RustKit view");
                }
            }
            Event::UserEvent(UserEvent::Layout {
                left,
                right_open,
                shelf: shelf_h,
            }) => {
                last_layout = (left, right_open, shelf_h);

                let size = window.inner_size();
                let width = size.width as f64;
                let height = size.height as f64;

                let chrome_h = 72.0;
                let right_sidebar_width = if right_open { 220.0 } else { 0.0 };

                let content_w = (width - left - right_sidebar_width).max(0.0);
                let content_h = (height - chrome_h - shelf_h).max(0.0);

                // Update WRY views
                let _ = chrome.set_bounds(rect(0.0, 0.0, width, chrome_h));
                let _ = shelf.set_bounds(rect(left, height - shelf_h, content_w, shelf_h));

                // Update RustKit content view
                let bounds = Bounds {
                    x: left as i32,
                    y: chrome_h as i32,
                    width: content_w as u32,
                    height: content_h as u32,
                };

                if let Err(e) = engine.resize_view(content_view_id, bounds) {
                    error!(?e, "Failed to resize RustKit view");
                }

                // Re-render after resize
                if let Err(e) = engine.render_view(content_view_id) {
                    error!(?e, "Failed to render RustKit view");
                }
            }
            Event::UserEvent(UserEvent::Exit) => {
                // Capture frame before exit if requested
                if let Some(ref path) = dump_frame_path {
                    if !frame_dumped {
                        info!(?path, "Dumping frame to file");
                        match engine.capture_frame(content_view_id, path) {
                            Ok(_) => {
                                info!("Frame captured successfully");
                                frame_dumped = true;
                            }
                            Err(e) => {
                                error!(?e, "Failed to capture frame");
                            }
                        }
                    }
                }

                let (left, right_open, shelf_h) = last_layout;
                let result = json!({
                    "status": "pass",
                    "elapsed_ms": start.elapsed().as_millis(),
                    "final_layout": {
                        "left_sidebar_width": left,
                        "right_sidebar_open": right_open,
                        "shelf_height": shelf_h
                    },
                    "frame_dumped": frame_dumped
                });
                println!("{}", result);
                *control_flow = ControlFlow::Exit;
            }
            Event::MainEventsCleared => {
                // Periodically re-render (at least once per frame)
                if let Err(e) = engine.render_view(content_view_id) {
                    // Only log first few errors to avoid spam
                    static RENDER_ERROR_COUNT: std::sync::atomic::AtomicU32 =
                        std::sync::atomic::AtomicU32::new(0);
                    let count = RENDER_ERROR_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if count < 3 {
                        error!(?e, "Failed to render RustKit view");
                    }
                }
            }
            _ => {}
        }
    });
}

