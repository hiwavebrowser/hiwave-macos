//! Headless frame capture tool for parity testing.
//!
//! This tool renders HTML files using RustKit's headless mode and exports:
//! - PPM frame capture
//! - Layout tree JSON
//! - Performance metrics
//!
//! Unlike hiwave-smoke, this does NOT require a display and can run in CI.

use clap::Parser;
use rustkit_engine::{EngineBuilder, EngineConfig};
use rustkit_viewhost::Bounds;
use serde::{Deserialize, Serialize};
use std::fs;
use tracing::error;

#[derive(Parser, Debug)]
#[command(name = "parity-capture")]
#[command(about = "Headless frame capture for parity testing")]
struct Args {
    /// Path to HTML file to render
    #[arg(long)]
    html_file: String,

    /// Viewport width
    #[arg(long, default_value = "1280")]
    width: u32,

    /// Viewport height
    #[arg(long, default_value = "800")]
    height: u32,

    /// Output path for PPM frame
    #[arg(long)]
    dump_frame: Option<String>,

    /// Output path for layout JSON
    #[arg(long)]
    dump_layout: Option<String>,

    /// Enable verbose output
    #[arg(long, short)]
    verbose: bool,
}

#[derive(Serialize, Deserialize)]
struct CaptureResult {
    status: String,
    html_file: String,
    width: u32,
    height: u32,
    frame_path: Option<String>,
    layout_path: Option<String>,
    layout_stats: Option<LayoutStats>,
    error: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct LayoutStats {
    total_boxes: u32,
    sized: u32,
    zero_size: u32,
    positioned: u32,
    at_origin: u32,
    sizing_rate: f32,
    positioning_rate: f32,
}

fn main() {
    let args = Args::parse();

    // Initialize tracing
    if args.verbose {
        tracing_subscriber::fmt()
            .with_env_filter("info")
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter("warn")
            .init();
    }

    let result = run_capture(&args);
    
    // Output JSON result
    println!("{}", serde_json::to_string(&result).unwrap());
    
    // Exit with appropriate code
    if result.status == "ok" {
        std::process::exit(0);
    } else {
        std::process::exit(1);
    }
}

fn run_capture(args: &Args) -> CaptureResult {
    // Read HTML file
    let html_content = match fs::read_to_string(&args.html_file) {
        Ok(content) => content,
        Err(e) => {
            return CaptureResult {
                status: "error".to_string(),
                html_file: args.html_file.clone(),
                width: args.width,
                height: args.height,
                frame_path: None,
                layout_path: None,
                layout_stats: None,
                error: Some(format!("Failed to read HTML file: {}", e)),
            };
        }
    };

    // Create engine with parity testing config (animations disabled)
    let engine_result = EngineBuilder::new()
        .with_config(EngineConfig::for_parity_testing())
        .user_agent("ParityCapture/1.0")
        .javascript_enabled(false)
        .build();

    let mut engine = match engine_result {
        Ok(e) => e,
        Err(e) => {
            return CaptureResult {
                status: "error".to_string(),
                html_file: args.html_file.clone(),
                width: args.width,
                height: args.height,
                frame_path: None,
                layout_path: None,
                layout_stats: None,
                error: Some(format!("Failed to create engine: {:?}", e)),
            };
        }
    };

    // Create headless view
    let bounds = Bounds {
        x: 0,
        y: 0,
        width: args.width,
        height: args.height,
    };

    let view_id = match engine.create_headless_view(bounds) {
        Ok(id) => id,
        Err(e) => {
            return CaptureResult {
                status: "error".to_string(),
                html_file: args.html_file.clone(),
                width: args.width,
                height: args.height,
                frame_path: None,
                layout_path: None,
                layout_stats: None,
                error: Some(format!("Failed to create headless view: {:?}", e)),
            };
        }
    };

    // Load HTML
    if let Err(e) = engine.load_html(view_id, &html_content) {
        return CaptureResult {
            status: "error".to_string(),
            html_file: args.html_file.clone(),
            width: args.width,
            height: args.height,
            frame_path: None,
            layout_path: None,
            layout_stats: None,
            error: Some(format!("Failed to load HTML: {:?}", e)),
        };
    }

    // Render
    if let Err(e) = engine.render_view(view_id) {
        return CaptureResult {
            status: "error".to_string(),
            html_file: args.html_file.clone(),
            width: args.width,
            height: args.height,
            frame_path: None,
            layout_path: None,
            layout_stats: None,
            error: Some(format!("Failed to render: {:?}", e)),
        };
    }

    // Capture frame if requested
    let frame_path = if let Some(ref path) = args.dump_frame {
        if let Err(e) = engine.capture_frame(view_id, path) {
            error!("Failed to capture frame: {:?}", e);
            None
        } else {
            Some(path.clone())
        }
    } else {
        None
    };

    // Export layout if requested
    let (layout_path, layout_stats) = if let Some(ref path) = args.dump_layout {
        match engine.export_layout_json(view_id, path) {
            Ok(()) => {
                // Read back the file to analyze
                match fs::read_to_string(path) {
                    Ok(layout_json) => {
                        let stats = analyze_layout_json(&layout_json);
                        (Some(path.clone()), stats)
                    }
                    Err(e) => {
                        error!("Failed to read layout file: {:?}", e);
                        (Some(path.clone()), None)
                    }
                }
            }
            Err(e) => {
                error!("Failed to export layout: {:?}", e);
                (None, None)
            }
        }
    } else {
        (None, None)
    };

    // Clean up
    let _ = engine.destroy_view(view_id);

    CaptureResult {
        status: "ok".to_string(),
        html_file: args.html_file.clone(),
        width: args.width,
        height: args.height,
        frame_path,
        layout_path,
        layout_stats,
        error: None,
    }
}

fn analyze_layout_json(json_str: &str) -> Option<LayoutStats> {
    let data: serde_json::Value = serde_json::from_str(json_str).ok()?;
    
    let mut stats = LayoutStats {
        total_boxes: 0,
        sized: 0,
        zero_size: 0,
        positioned: 0,
        at_origin: 0,
        sizing_rate: 0.0,
        positioning_rate: 0.0,
    };

    fn walk(node: &serde_json::Value, stats: &mut LayoutStats) {
        if let Some(rect) = node.get("content_rect").or(node.get("rect")) {
            let x = rect.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let y = rect.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let w = rect.get("width").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let h = rect.get("height").and_then(|v| v.as_f64()).unwrap_or(0.0);

            stats.total_boxes += 1;

            if x != 0.0 || y != 0.0 {
                stats.positioned += 1;
            } else {
                stats.at_origin += 1;
            }

            if w > 0.0 && h > 0.0 {
                stats.sized += 1;
            } else {
                stats.zero_size += 1;
            }
        }

        if let Some(children) = node.get("children").and_then(|v| v.as_array()) {
            for child in children {
                walk(child, stats);
            }
        }
    }

    if let Some(root) = data.get("root") {
        walk(root, &mut stats);
    }

    let total = stats.total_boxes.max(1) as f32;
    stats.sizing_rate = stats.sized as f32 / total;
    stats.positioning_rate = stats.positioned as f32 / total;

    Some(stats)
}

