//! What a compositing layer costs (LAT13): every opacity or filter group is
//! drawn into a layer and composited back. Before LAT13 each layer took four
//! `queue.submit` calls (flush under it, clear, flush into it, composite),
//! and 50 opacity layers took 15.3 ms a frame in a 400x300 headless view
//! against 0.47 ms with none.
//!
//! The timing table is an ignored test:
//! `cargo test --release -p rustkit-renderer --lib -- --ignored layer_cost_table --nocapture`.
//! The GPU tests skip, loudly, where no adapter exists.

use super::*;
use std::time::Instant;

const VIEW_W: u32 = 400;
const VIEW_H: u32 = 300;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;

struct Gpu {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
}

fn gpu() -> Option<Gpu> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))?;
    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("layer cost test device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
        },
        None,
    ))
    .ok()?;
    Some(Gpu {
        device: Arc::new(device),
        queue: Arc::new(queue),
    })
}

fn renderer(gpu: &Gpu) -> Renderer {
    let mut r = Renderer::new(gpu.device.clone(), gpu.queue.clone(), FORMAT).expect("renderer");
    r.set_viewport_size(VIEW_W, VIEW_H);
    r
}

fn fill(x: f32, y: f32, w: f32, h: f32, color: Color) -> DisplayCommand {
    DisplayCommand::SolidColor(color, Rect::new(x, y, w, h))
}

/// A white page and `n` half-transparent boxes, each its own opacity group,
/// in a grid of 10 columns (the engine's list for `n` boxes with
/// `opacity: 0.5`).
fn opacity_page(n: usize) -> Vec<DisplayCommand> {
    let mut list = vec![fill(0.0, 0.0, VIEW_W as f32, VIEW_H as f32, Color::new(255, 255, 255, 1.0))];
    for i in 0..n {
        let (x, y) = ((i % 10) as f32 * 40.0, (i / 10) as f32 * 40.0);
        list.push(DisplayCommand::PushOpacity(0.5));
        list.push(fill(x + 2.0, y + 2.0, 36.0, 36.0, Color::new((i * 5 % 256) as u8, 80, 200, 1.0)));
        list.push(DisplayCommand::PopOpacity);
    }
    list
}

/// Milliseconds for one frame: CPU work plus waiting for the GPU.
fn time_frame(gpu: &Gpu, r: &mut Renderer, view: &wgpu::TextureView, list: &[DisplayCommand]) -> f64 {
    let t = Instant::now();
    r.execute(list, view).expect("execute");
    gpu.device.poll(wgpu::Maintain::Wait);
    t.elapsed().as_secs_f64() * 1e3
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

/// Median frame time over `frames` frames after a few warm-up frames.
fn frame_ms(gpu: &Gpu, list: &[DisplayCommand], frames: usize) -> (f64, FrameStats) {
    let mut r = renderer(gpu);
    let (_texture, view) = screenshot::create_offscreen_target(&gpu.device, VIEW_W, VIEW_H, FORMAT);
    for _ in 0..5 {
        time_frame(gpu, &mut r, &view, list);
    }
    let ms = median((0..frames).map(|_| time_frame(gpu, &mut r, &view, list)).collect());
    (ms, r.last_frame_stats())
}

#[test]
#[ignore = "timing table; run with --release --ignored --nocapture"]
fn layer_cost_table() {
    let Some(gpu) = gpu() else {
        eprintln!("SKIP layer_cost_table: no GPU adapter");
        return;
    };
    println!("{:>8} {:>10} {:>8}", "layers", "ms/frame", "submits");
    for n in [0usize, 1, 10, 50, 100] {
        let (ms, stats) = frame_ms(&gpu, &opacity_page(n), 50);
        println!("{n:>8} {ms:>10.3} {:>8}", stats.submits);
    }
}

/// LAT13: a frame's layers are recorded into one command encoder and
/// submitted once, however many groups the list opens.
#[test]
fn fifty_opacity_layers_are_submitted_once() {
    let Some(gpu) = gpu() else {
        eprintln!("SKIP fifty_opacity_layers_are_submitted_once: no GPU adapter");
        return;
    };
    let mut r = renderer(&gpu);
    let (_texture, view) = screenshot::create_offscreen_target(&gpu.device, VIEW_W, VIEW_H, FORMAT);
    for n in [0usize, 1, 50] {
        r.execute(&opacity_page(n), &view).expect("execute");
        assert_eq!(r.last_frame_stats().submits, 1, "{n} opacity layers");
    }
}

/// LAT13: 50 opacity layers in a 400x300 view took 15.3 ms a frame in the
/// engine (11 ms here, release) with four submits each. The bound is loose
/// on purpose (a debug build on a loaded machine): it catches a return to
/// per-layer submits, not a small slowdown.
#[test]
fn fifty_opacity_layers_fit_a_frame_budget() {
    let Some(gpu) = gpu() else {
        eprintln!("SKIP fifty_opacity_layers_fit_a_frame_budget: no GPU adapter");
        return;
    };
    let (ms, _) = frame_ms(&gpu, &opacity_page(50), 21);
    let (base, _) = frame_ms(&gpu, &opacity_page(0), 21);
    println!("50 layers {ms:.3} ms/frame, none {base:.3} ms/frame");
    assert!(ms < base + BUDGET_MS, "50 opacity layers took {ms:.3} ms/frame (none: {base:.3} ms)");
}

/// What 50 layers may add to a frame with none, in a debug build.
const BUDGET_MS: f64 = 4.0;
