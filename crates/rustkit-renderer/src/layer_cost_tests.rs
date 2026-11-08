//! What a compositing layer costs (LAT13): every opacity or filter group is
//! drawn into a layer and composited back. Before LAT13 each layer took four
//! `queue.submit` calls (flush under it, clear, flush into it, composite),
//! and 50 opacity layers took 15.3 ms a frame in a 400x300 headless view
//! against 0.47 ms with none.
//!
//! Where the time went (release, this table, Apple M-series): 11.2 ms for
//! 50 layers, 151 submits; the CPU side 5.1 ms (1.1 ms in `queue.submit`,
//! the rest a vertex and an index buffer per draw), the GPU side about
//! 50 us a render pass whatever it draws (150 passes); no layer texture was
//! allocated per frame (they were already kept by depth). One submit alone
//! made it slower (the GPU then starts at the end of the list): the passes
//! and the buffers were the cost. Now: one pass per used layer texture and
//! one for the frame, one buffer for the frame's vertices and indices, one
//! submit; 50 layers 1.6 ms against 1.55 ms with none (both mostly the
//! wait for the GPU to report back).
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

/// LAT13: 50 opacity layers in a 400x300 view took 15.3 ms a frame in a
/// debug build (11.2 ms release) with four submits and four passes each;
/// now about 0.4 ms more than none in a debug build. The bound is loose on
/// purpose (a debug build on a loaded machine): it catches a return to a
/// pass per layer, not a small slowdown.
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


fn text(s: &str, x: f32, y: f32, color: Color) -> DisplayCommand {
    DisplayCommand::Text {
        text: s.to_string(),
        x,
        y,
        color,
        font_size: 14.0,
        font_family: "sans-serif".to_string(),
        font_weight: 400,
        font_style: 0,
        advances: Some(vec![7.0; s.chars().count()]),
        ascent: Some(13.0),
        run: None,
    }
}

fn image(url: &str, x: f32, y: f32, w: f32, h: f32) -> DisplayCommand {
    let r = Rect::new(x, y, w, h);
    DisplayCommand::Image {
        url: url.to_string(),
        src_rect: None,
        dest_rect: r,
        object_fit: ObjectFit::Fill,
        content_box: r,
        object_position: (0.5, 0.5),
        object_position_offset: (0.0, 0.0),
        opacity: 1.0,
        current_color: Color::new(0, 0, 0, 1.0),
    }
}

// invert(1), then a matrix that copies red into every channel (CSS
// `filter` matrices as `rustkit_layout::filter` builds them).
const INVERT: [f32; 20] = [
    -1.0, 0.0, 0.0, 0.0, 1.0, 0.0, -1.0, 0.0, 0.0, 1.0, 0.0, 0.0, -1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0,
];
const SEPIA_ISH: [f32; 20] = [
    0.4, 0.7, 0.2, 0.0, 0.0, 0.3, 0.6, 0.2, 0.0, 0.0, 0.2, 0.5, 0.1, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0,
];

fn c(r: u8, g: u8, b: u8, a: f32) -> Color {
    Color::new(r, g, b, a)
}

use DisplayCommand::{PopClip, PopFilter, PopOpacity, PopTransform, PushClip, PushFilter, PushOpacity};

/// Lists that take every turn of the layer paths: nesting, overlapping and
/// separate siblings (more overlapping ones at a depth than there are layer
/// slots, so the frame falls back), filters, text, images, clips,
/// rotation, fractional edges, groups off screen and empty, a group left
/// open at the end.
fn layer_cases() -> Vec<(&'static str, Vec<DisplayCommand>)> {
    let white = c(255, 255, 255, 1.0);
    let bg = || fill(0.0, 0.0, VIEW_W as f32, VIEW_H as f32, c(240, 236, 220, 1.0));
    let mut cases = vec![("fifty separate", opacity_page(50))];
    cases.push((
        "nested three deep, overlapping their parents",
        vec![
            bg(),
            PushOpacity(0.8),
            fill(10.0, 10.0, 200.0, 150.0, c(200, 30, 30, 1.0)),
            PushOpacity(0.6),
            fill(60.0, 40.0, 200.0, 150.0, c(30, 200, 30, 1.0)),
            PushOpacity(0.5),
            fill(110.0, 70.0, 200.0, 150.0, c(30, 30, 200, 1.0)),
            text("inner text", 120.0, 80.0, white),
            PopOpacity,
            fill(20.0, 200.0, 80.0, 30.0, c(0, 0, 0, 0.5)),
            PopOpacity,
            PopOpacity,
        ],
    ));
    for n in [3usize, 4, 5, 9] {
        let mut list = vec![bg()];
        for i in 0..n {
            list.push(PushOpacity(0.3 + 0.05 * i as f32));
            list.push(fill(20.0 + 15.0 * i as f32, 20.0 + 10.0 * i as f32, 150.0, 100.0, c((40 * i % 256) as u8, 90, 160, 1.0)));
            list.push(text("over", 30.0 + 15.0 * i as f32, 30.0, c(0, 0, 0, 1.0)));
            list.push(PopOpacity);
        }
        let name = match n {
            3 => "3 overlapping",
            4 => "4 overlapping",
            5 => "5 overlapping",
            _ => "9 overlapping",
        };
        cases.push((name, list));
    }
    cases.push((
        "filters, faded inside and out, overlapping",
        vec![
            bg(),
            PushFilter(INVERT),
            fill(10.0, 10.0, 120.0, 120.0, c(200, 100, 50, 1.0)),
            PushOpacity(0.5),
            fill(60.0, 60.0, 120.0, 120.0, c(20, 160, 220, 1.0)),
            PopOpacity,
            PopFilter,
            PushOpacity(0.7),
            PushFilter(SEPIA_ISH),
            fill(100.0, 30.0, 150.0, 90.0, c(10, 200, 90, 0.8)),
            image("a.png", 150.0, 50.0, 40.0, 40.0),
            PopFilter,
            PopOpacity,
            PushFilter(INVERT),
            PopFilter,
            PushFilter(SEPIA_ISH),
            fill(300.0, 200.0, 60.0, 60.0, c(250, 20, 20, 1.0)),
            PopFilter,
        ],
    ));
    cases.push((
        "text, images and a clip inside groups",
        vec![
            bg(),
            text("under the groups", 10.0, 5.0, c(0, 0, 0, 1.0)),
            PushOpacity(0.5),
            image("a.png", 5.0, 10.0, 60.0, 60.0),
            text("faded text", 10.0, 30.0, c(0, 0, 0, 1.0)),
            PopOpacity,
            PushClip(Rect::new(100.0, 100.0, 80.0, 50.0)),
            PushOpacity(0.4),
            fill(80.0, 80.0, 200.0, 200.0, c(255, 0, 128, 1.0)),
            image("b.png", 120.0, 90.0, 100.0, 100.0),
            PopOpacity,
            PopClip,
            fill(150.0, 20.0, 60.0, 60.0, c(0, 100, 0, 0.7)),
        ],
    ));
    cases.push((
        "rotated and fractional groups",
        vec![
            bg(),
            DisplayCommand::PushTransform {
                matrix: [0.866, 0.5, -0.5, 0.866, 0.0, 0.0],
                origin: (200.0, 150.0),
            },
            PushOpacity(0.6),
            fill(150.3, 100.7, 100.4, 80.2, c(30, 60, 200, 1.0)),
            text("tilted", 160.0, 120.0, white),
            PopOpacity,
            PopTransform,
            PushOpacity(0.55),
            fill(10.25, 250.5, 33.3, 20.6, c(200, 10, 10, 1.0)),
            PopOpacity,
            PushOpacity(0.55),
            fill(43.6, 250.5, 33.3, 20.6, c(10, 10, 200, 1.0)),
            PopOpacity,
        ],
    ));
    cases.push((
        "off screen, empty, and one left open",
        vec![
            PushOpacity(0.5),
            PopOpacity,
            bg(),
            PushOpacity(0.5),
            fill(-200.0, -200.0, 50.0, 50.0, c(0, 0, 0, 1.0)),
            PopOpacity,
            PushOpacity(0.5),
            fill(380.0, 280.0, 100.0, 100.0, c(0, 120, 0, 1.0)),
            PopOpacity,
            PopOpacity,
            PushOpacity(0.3),
            fill(50.0, 50.0, 100.0, 100.0, c(90, 0, 90, 1.0)),
        ],
    ));
    cases.push(("an empty group only", vec![PushOpacity(0.5), PopOpacity]));
    cases
}

/// How a frame is drawn: the deferred path, the pass-per-layer path, and
/// that path with each pass submitted on its own (what develop did).
#[derive(Clone, Copy, Debug)]
enum Way {
    Deferred,
    PassPerLayer,
    SubmitPerPass,
}

fn draw_case(gpu: &Gpu, list: &[DisplayCommand], way: Way, transparent: bool, scroll: (f32, f32)) -> Vec<u8> {
    let mut r = renderer(gpu);
    let px: Vec<u8> = [30u8, 120, 200, 255].repeat(16 * 16);
    let half: Vec<u8> = [200u8, 50, 20, 128].repeat(16 * 16);
    r.upload_image("a.png", 16, 16, &px).unwrap();
    r.upload_image("b.png", 16, 16, &half).unwrap();
    r.set_clear_color([0.2, 0.4, 0.6, 1.0]);
    r.set_transparent(transparent);
    r.legacy_layers = !matches!(way, Way::Deferred);
    r.immediate_submits = matches!(way, Way::SubmitPerPass);
    r.set_scroll_offset(scroll);
    let (texture, view) = screenshot::create_offscreen_target(&gpu.device, VIEW_W, VIEW_H, FORMAT);
    r.execute(list, &view).expect("execute");
    let readback = screenshot::GpuReadbackBuffer::new(&gpu.device, VIEW_W, VIEW_H);
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    readback.copy_from_texture(&mut encoder, &texture);
    gpu.queue.submit(std::iter::once(encoder.finish()));
    readback.read_data_sync(&gpu.device).expect("readback")
}

/// LAT13: the deferred path draws the pixels the pass-per-layer path drew,
/// and that path the pixels it drew with a submit per pass, exactly.
#[test]
fn layered_frames_are_pixel_identical_on_every_path() {
    let Some(gpu) = gpu() else {
        eprintln!("SKIP layered_frames_are_pixel_identical_on_every_path: no GPU adapter");
        return;
    };
    for (what, list) in layer_cases() {
        for (transparent, scroll) in [(false, (0.0, 0.0)), (true, (0.0, 0.0)), (false, (13.0, 41.5))] {
            let want = draw_case(&gpu, &list, Way::SubmitPerPass, transparent, scroll);
            for way in [Way::PassPerLayer, Way::Deferred] {
                let got = draw_case(&gpu, &list, way, transparent, scroll);
                let diff = want.iter().zip(&got).filter(|(a, b)| a != b).count();
                assert!(
                    diff == 0,
                    "{what} (transparent {transparent}, scroll {scroll:?}): {way:?} differs in {diff} bytes"
                );
            }
        }
    }
}

/// The overlapping cases really take the turns they are there for.
#[test]
fn overlapping_groups_share_no_slot_and_fall_back_past_the_slots() {
    let Some(gpu) = gpu() else {
        eprintln!("SKIP overlapping_groups_share_no_slot_and_fall_back_past_the_slots: no GPU adapter");
        return;
    };
    let cases = layer_cases();
    let list = |name: &str| cases.iter().find(|(w, _)| *w == name).unwrap().1.clone();
    let mut r = renderer(&gpu);
    let (_texture, view) = screenshot::create_offscreen_target(&gpu.device, VIEW_W, VIEW_H, FORMAT);
    r.execute(&list("fifty separate"), &view).unwrap();
    assert_eq!(r.layer_slots[0].len(), 1, "50 separate boxes share one layer texture");
    r.execute(&list("4 overlapping"), &view).unwrap();
    assert_eq!(r.layer_slots[0].len(), LAYER_SLOTS_PER_DEPTH, "each overlapping group its own");
    assert_eq!(r.last_frame_stats().submits, 1);
    r.execute(&list("5 overlapping"), &view).unwrap();
    assert_eq!(r.layer_slots[0].len(), LAYER_SLOTS_PER_DEPTH, "no more than that");
    assert_eq!(r.last_frame_stats().submits, 1, "the fallback frame is still one submit");
}
