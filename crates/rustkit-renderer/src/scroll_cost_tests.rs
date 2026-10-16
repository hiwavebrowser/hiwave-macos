//! What a scrolled frame costs, on synthetic display lists (the threads-P3
//! follow-up: a 20,000-command page took 426 ms per scrolled frame on the
//! Mac), and proof that the fast path keeps paint order.
//!
//! The timing table is an ignored test:
//! `cargo test --release -p rustkit-renderer --lib -- --ignored scroll_cost_table --nocapture`
//! (`SCROLL_COST_SIZES=1000,5000` picks the sizes). The GPU tests skip,
//! loudly, where no adapter exists.

use super::*;
use std::time::Instant;

const VIEW_W: u32 = 1024;
const VIEW_H: u32 = 768;
const ROW_H: f32 = 20.0;

struct Gpu {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    adapter: String,
}

/// One device per test (the renderer's tests share no GPU state).
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
    let name = adapter.get_info().name;
    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("scroll cost test device"),
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
        adapter: name,
    })
}

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;

fn renderer(gpu: &Gpu) -> Renderer {
    let mut r = Renderer::new(gpu.device.clone(), gpu.queue.clone(), FORMAT).expect("renderer");
    r.set_viewport_size(VIEW_W, VIEW_H);
    r
}

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

fn fill(x: f32, y: f32, w: f32, h: f32, color: Color) -> DisplayCommand {
    DisplayCommand::SolidColor(color, Rect::new(x, y, w, h))
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

#[derive(Clone, Copy, Debug)]
#[repr(usize)]
enum Mix {
    /// Paragraph rows of text, an underline now and then.
    Text,
    /// Every row a small background (badge, table cell) and its text; one
    /// row in fifty has a box that lands on the row above's text.
    Backgrounds,
    /// `Backgrounds` with no box over earlier text: nothing ever flushes,
    /// so every fill is checked against every glyph batched so far.
    Table,
    /// Thumbnail, caption and a separator line per row.
    Images,
}

const LINE: &str = "The quick brown fox jumps over the lazy dog 0123";

/// A page of about `n` commands, rows `ROW_H` apart, a page background first.
fn page(mix: Mix, n: usize) -> Vec<DisplayCommand> {
    let ink = Color::new(20, 20, 20, 1.0);
    let mut list = vec![fill(
        0.0,
        0.0,
        VIEW_W as f32,
        0.0,
        Color::new(255, 255, 255, 1.0),
    )];
    let mut row = 0usize;
    while list.len() < n {
        let y = row as f32 * ROW_H;
        match mix {
            Mix::Text => {
                list.push(text(LINE, 16.0, y, ink));
                if row % 7 == 3 {
                    list.push(DisplayCommand::TextDecoration {
                        x: 16.0,
                        y: y + 16.0,
                        width: 120.0,
                        thickness: 1.0,
                        color: ink,
                        style: rustkit_layout::TextDecorationStyleValue::Solid,
                    });
                }
            }
            Mix::Backgrounds | Mix::Table => {
                let tint = Color::new(200 + (row % 5) as u8 * 10, 230, 240, 1.0);
                list.push(fill(10.0, y + 1.0, 80.0, ROW_H - 2.0, tint));
                list.push(text("cell", 14.0, y, ink));
                list.push(fill(
                    100.0,
                    y + 1.0,
                    500.0,
                    ROW_H - 2.0,
                    Color::new(245, 245, 250, 1.0),
                ));
                list.push(text(LINE, 104.0, y, ink));
                if matches!(mix, Mix::Backgrounds) && row % 50 == 49 {
                    // A positioned box over the row above's text.
                    list.push(fill(
                        300.0,
                        y - 12.0,
                        60.0,
                        16.0,
                        Color::new(250, 80, 80, 1.0),
                    ));
                }
            }
            Mix::Images => {
                list.push(image(
                    if row.is_multiple_of(2) {
                        "a.png"
                    } else {
                        "b.png"
                    },
                    10.0,
                    y + 2.0,
                    16.0,
                    16.0,
                ));
                list.push(text(LINE, 32.0, y, ink));
                list.push(fill(
                    0.0,
                    y + ROW_H - 1.0,
                    VIEW_W as f32,
                    1.0,
                    Color::new(220, 220, 220, 1.0),
                ));
            }
        }
        row += 1;
    }
    let height = row as f32 * ROW_H;
    list[0] = fill(
        0.0,
        0.0,
        VIEW_W as f32,
        height,
        Color::new(255, 255, 255, 1.0),
    );
    list
}

trait PushGet<T> {
    fn push_get(&mut self, item: T) -> &mut T;
}

impl<T> PushGet<T> for Vec<T> {
    fn push_get(&mut self, item: T) -> &mut T {
        self.push(item);
        self.last_mut().unwrap()
    }
}

fn page_height(list: &[DisplayCommand]) -> f32 {
    match list[0] {
        DisplayCommand::SolidColor(_, r) => r.height,
        _ => unreachable!(),
    }
}

fn upload_images(r: &mut Renderer) {
    let px: Vec<u8> = [30u8, 120, 200, 255].repeat(16 * 16);
    r.upload_image("a.png", 16, 16, &px).unwrap();
    r.upload_image("b.png", 16, 16, &px).unwrap();
}

/// The frame's pixels (BGRA).
fn draw(gpu: &Gpu, r: &mut Renderer, list: &[DisplayCommand], scroll: (f32, f32)) -> Vec<u8> {
    let (texture, view) = screenshot::create_offscreen_target(&gpu.device, VIEW_W, VIEW_H, FORMAT);
    r.set_scroll_offset(scroll);
    r.execute(list, &view).expect("execute");
    r.set_scroll_offset((0.0, 0.0));
    let readback = screenshot::GpuReadbackBuffer::new(&gpu.device, VIEW_W, VIEW_H);
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    readback.copy_from_texture(&mut encoder, &texture);
    gpu.queue.submit(std::iter::once(encoder.finish()));
    readback.read_data_sync(&gpu.device).expect("readback")
}

/// Milliseconds for one frame, CPU work plus waiting for the GPU; `NaN`
/// when the frame fails (a batch past wgpu's buffer limit).
fn time_frame(
    gpu: &Gpu,
    r: &mut Renderer,
    view: &wgpu::TextureView,
    list: &[DisplayCommand],
    scroll: (f32, f32),
) -> f64 {
    r.set_scroll_offset(scroll);
    gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let t = Instant::now();
    let drawn = r.execute(list, view);
    gpu.device.poll(wgpu::Maintain::Wait);
    let ms = t.elapsed().as_secs_f64() * 1e3;
    let invalid = pollster::block_on(gpu.device.pop_error_scope());
    r.set_scroll_offset((0.0, 0.0));
    if drawn.is_err() || invalid.is_some() {
        f64::NAN
    } else {
        ms
    }
}

fn median(mut v: Vec<f64>) -> f64 {
    if v.iter().any(|ms| ms.is_nan()) {
        return f64::NAN;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

/// The commands of `list` with a row inside the viewport at `scroll_y`:
/// the work a frame cannot avoid.
fn visible_only(list: &[DisplayCommand], scroll_y: f32) -> Vec<DisplayCommand> {
    let top = |c: &DisplayCommand| match c {
        DisplayCommand::SolidColor(_, r) => (r.y, r.y + r.height),
        DisplayCommand::Text { y, .. } => (*y, *y + ROW_H),
        DisplayCommand::TextDecoration { y, thickness, .. } => (*y, *y + thickness),
        DisplayCommand::Image { dest_rect, .. } => (dest_rect.y, dest_rect.y + dest_rect.height),
        _ => (f32::MIN, f32::MAX),
    };
    list.iter()
        .filter(|c| {
            let (y0, y1) = top(c);
            y1 > scroll_y && y0 < scroll_y + VIEW_H as f32
        })
        .cloned()
        .collect()
}

#[test]
#[ignore = "timing table; run with --release --ignored --nocapture"]
fn scroll_cost_table() {
    let Some(gpu) = gpu() else {
        eprintln!("SKIP scroll_cost_table: no GPU adapter");
        return;
    };
    let sizes: Vec<usize> = std::env::var("SCROLL_COST_SIZES")
        .ok()
        .map(|s| s.split(',').filter_map(|n| n.trim().parse().ok()).collect())
        .unwrap_or_else(|| vec![1_000, 5_000, 20_000, 80_000]);
    let reps: usize = std::env::var("SCROLL_COST_REPS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5);
    println!("adapter: {}", gpu.adapter);
    println!("| mix | path | commands | frame ms (scroll 0) | scrolled frame ms | visible-only ms | overlap checks | overlap comparisons | overlap ms | overlap flushes | batch flushes | submits | quads | quads off-screen | culled |");
    println!("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|");
    let mixes = std::env::var("SCROLL_COST_MIXES").unwrap_or_default();
    // `SCROLL_COST_PATHS=after` skips the slow reference path.
    let paths = std::env::var("SCROLL_COST_PATHS").unwrap_or_else(|_| "before,after".into());
    for mix in [Mix::Text, Mix::Backgrounds, Mix::Table, Mix::Images] {
        if !mixes.is_empty() && !mixes.contains(&format!("{mix:?}")) {
            continue;
        }
        for &n in &sizes {
            let list = page(mix, n);
            for (path, reference) in [("before", true), ("after", false)] {
                if !paths.contains(path) {
                    continue;
                }
                let mut r = renderer(&gpu);
                r.reference = reference;
                upload_images(&mut r);
                let (_t, view) =
                    screenshot::create_offscreen_target(&gpu.device, VIEW_W, VIEW_H, FORMAT);
                let mid = ((page_height(&list) - VIEW_H as f32) / 2.0)
                    .max(0.0)
                    .round();
                // Warm the glyph atlas.
                time_frame(&gpu, &mut r, &view, &list, (0.0, 0.0));
                let at0 = median(
                    (0..reps)
                        .map(|_| time_frame(&gpu, &mut r, &view, &list, (0.0, 0.0)))
                        .collect(),
                );
                // A wheel scroll re-presents the same list at a new offset.
                let scrolled = median(
                    (0..reps)
                        .map(|i| {
                            time_frame(&gpu, &mut r, &view, &list, (0.0, mid + (i as f32) * 40.0))
                        })
                        .collect(),
                );
                let visible = visible_only(&list, mid);
                time_frame(&gpu, &mut r, &view, &visible, (0.0, mid));
                let vis = median(
                    (0..reps)
                        .map(|_| time_frame(&gpu, &mut r, &view, &visible, (0.0, mid)))
                        .collect(),
                );
                r.set_measure(true);
                time_frame(&gpu, &mut r, &view, &list, (0.0, mid));
                let s = r.last_frame_stats();
                println!(
                    "| {mix:?} | {path} | {} | {at0:.1} | {scrolled:.1} | {vis:.1} | {} | {} | {:.1} | {} | {} | {} | {} | {} | {} |",
                    list.len(),
                    s.overlap_checks,
                    s.overlap_comparisons,
                    s.overlap_ns as f64 / 1e6,
                    s.overlap_flushes,
                    s.batch_flushes,
                    s.submits,
                    s.quads,
                    s.quads_offscreen,
                    s.culled,
                );
            }
        }
    }
}

/// A page that exercises what culling must get right: a fixed header, an
/// `overflow: hidden` box whose content is mostly clipped, a transform that
/// pulls far-down content into view, an opacity group, a rotated box, a
/// positioned box over earlier text, and `n` rows of mixed content.
fn kitchen_sink(n: usize) -> Vec<DisplayCommand> {
    let ink = Color::new(20, 20, 20, 1.0);
    let mut list = page(Mix::Backgrounds, n);
    let height = page_height(&list);
    list.extend([
        // Content 3000px down, translated up into the first screen.
        DisplayCommand::PushTransform {
            matrix: [1.0, 0.0, 0.0, 1.0, 0.0, -3000.0],
            origin: (0.0, 0.0),
        },
        fill(620.0, 3100.0, 200.0, 40.0, Color::new(40, 160, 90, 1.0)),
        text(LINE, 624.0, 3110.0, Color::new(255, 255, 255, 1.0)),
        DisplayCommand::PopTransform,
        // A rotated card: its bounds are tested on the mapped corners.
        DisplayCommand::PushTransform {
            matrix: [0.0, 1.0, -1.0, 0.0, 0.0, 0.0],
            origin: (700.0, 400.0),
        },
        fill(650.0, 380.0, 120.0, 40.0, Color::new(90, 60, 200, 1.0)),
        text("rotated", 660.0, 390.0, Color::new(255, 255, 255, 1.0)),
        DisplayCommand::PopTransform,
        // A clipped scroller: 200 rows inside a 300px box.
        DisplayCommand::PushClip(Rect::new(650.0, 500.0, 300.0, 120.0)),
    ]);
    for i in 0..200 {
        let y = 500.0 + i as f32 * ROW_H;
        list.push(fill(
            650.0,
            y,
            300.0,
            ROW_H - 1.0,
            Color::new(255, 240 - (i % 4) as u8 * 20, 200, 1.0),
        ));
        list.push(text(LINE, 654.0, y, ink));
    }
    list.push(DisplayCommand::PopClip);
    // An opacity group across the page.
    list.push(DisplayCommand::PushOpacity(0.5));
    for i in 0..60 {
        let y = i as f32 * (height / 60.0);
        list.push(fill(800.0, y, 150.0, 30.0, Color::new(0, 120, 255, 1.0)));
        list.push(text("faded", 804.0, y + 4.0, ink));
    }
    list.push(DisplayCommand::PopOpacity);
    // Every 400px: an image, a caption on it, a badge over both, then an
    // image over earlier text; and text right of the viewport whose
    // negative letter-spacing walks it back on screen.
    let mut y = 60.0;
    while y < height {
        list.push(image("a.png", 300.0, y, 120.0, 80.0));
        list.push(text(
            "caption",
            304.0,
            y + 50.0,
            Color::new(255, 255, 255, 1.0),
        ));
        list.push(fill(
            390.0,
            y - 6.0,
            40.0,
            24.0,
            Color::new(220, 0, 60, 1.0),
        ));
        list.push(text("seen through", 440.0, y + 20.0, ink));
        list.push(image("b.png", 470.0, y + 18.0, 30.0, 30.0));
        if let DisplayCommand::Text {
            advances: Some(advances),
            ..
        } = list.push_get(text(LINE, 1400.0, y + 100.0, ink))
        {
            advances.iter_mut().for_each(|a| *a = -14.0);
        }
        y += 400.0;
    }
    // A fixed header, drawn last so it is on top.
    list.extend([
        DisplayCommand::PushViewportFixed,
        fill(0.0, 0.0, VIEW_W as f32, 40.0, Color::new(30, 30, 40, 1.0)),
        text("Fixed header", 16.0, 12.0, Color::new(255, 255, 255, 1.0)),
        fill(
            0.0,
            VIEW_H as f32 - 24.0,
            VIEW_W as f32,
            24.0,
            Color::new(240, 200, 0, 1.0),
        ),
        DisplayCommand::PopViewportFixed,
    ]);
    list
}

fn reference_draw(gpu: &Gpu, list: &[DisplayCommand], scroll: (f32, f32)) -> (Vec<u8>, FrameStats) {
    let mut r = renderer(gpu);
    upload_images(&mut r);
    r.reference = true;
    let px = draw(gpu, &mut r, list, scroll);
    (px, r.last_frame_stats())
}

fn fast_draw(gpu: &Gpu, list: &[DisplayCommand], scroll: (f32, f32)) -> (Vec<u8>, FrameStats) {
    let mut r = renderer(gpu);
    upload_images(&mut r);
    let px = draw(gpu, &mut r, list, scroll);
    (px, r.last_frame_stats())
}

#[test]
fn culled_frames_are_byte_identical_to_unculled_ones_at_every_scroll() {
    let Some(gpu) = gpu() else {
        eprintln!("SKIP: no GPU adapter");
        return;
    };
    let mut lists: Vec<(&str, Vec<DisplayCommand>)> = vec![("kitchen sink", kitchen_sink(3_000))];
    for mix in [Mix::Text, Mix::Backgrounds, Mix::Table, Mix::Images] {
        lists.push((
            ["text", "backgrounds", "table", "images"][mix as usize],
            page(mix, 2_000),
        ));
    }
    for (name, list) in &lists {
        let end = page_height(list) - VIEW_H as f32;
        for scroll in [
            (0.0, 0.0),
            (0.0, 777.0),
            (0.0, (end / 2.0).round()),
            (0.0, end),
            (13.0, 500.5),
            (0.0, -50.0),
        ] {
            let (want, want_stats) = reference_draw(&gpu, list, scroll);
            let (got, got_stats) = fast_draw(&gpu, list, scroll);
            let differ = want
                .chunks_exact(4)
                .zip(got.chunks_exact(4))
                .filter(|(a, b)| a != b)
                .count();
            assert_eq!(differ, 0, "{name} at {scroll:?}: {differ} pixels differ");
            assert_eq!(want_stats.culled, 0);
            assert!(
                got_stats.culled > 0 || list.len() < 100,
                "{name} at {scroll:?}: nothing culled"
            );
            // The grid finds the same overlaps the scan found wherever
            // nothing was culled away; culling only drops flushes caused
            // by off-screen geometry.
            assert!(got_stats.overlap_flushes <= want_stats.overlap_flushes);
        }
    }
}

#[test]
fn paint_order_is_kept_for_interleaved_fills_and_text() {
    let Some(gpu) = gpu() else {
        eprintln!("SKIP: no GPU adapter");
        return;
    };
    // Rows of: a background, text on it, then a box over that text (a
    // positioned box painting later), then text on the box. Every box
    // over text must flush, and the pixels must be the scan's.
    let ink = Color::new(0, 0, 0, 1.0);
    let red = Color::new(255, 0, 0, 1.0);
    let mut list = vec![fill(
        0.0,
        0.0,
        VIEW_W as f32,
        VIEW_H as f32,
        Color::new(255, 255, 255, 1.0),
    )];
    let rows = 30;
    for i in 0..rows {
        let y = 10.0 + i as f32 * 24.0;
        list.push(fill(10.0, y, 600.0, 22.0, Color::new(230, 230, 255, 1.0)));
        list.push(text(
            "MMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMM",
            14.0,
            y + 2.0,
            ink,
        ));
        list.push(fill(100.0, y, 60.0, 22.0, red));
        list.push(text("ok", 104.0, y + 2.0, Color::new(255, 255, 255, 1.0)));
    }
    let (want, want_stats) = reference_draw(&gpu, &list, (0.0, 0.0));
    let (got, got_stats) = fast_draw(&gpu, &list, (0.0, 0.0));
    assert_eq!(want, got, "the grid changed the pixels");
    assert_eq!(got_stats.overlap_flushes, want_stats.overlap_flushes);
    assert!(
        got_stats.overlap_flushes >= rows as u64,
        "every red box lands on text: {got_stats:?}"
    );
    assert!(
        got_stats.overlap_comparisons < want_stats.overlap_comparisons
            || want_stats.overlap_comparisons < 100
    );
    // The red box is on top of the black text under it: inside the box,
    // away from its own white text, no pixel is black. BGRA.
    for i in 0..rows {
        let y = (10.0 + i as f32 * 24.0) as usize + 10;
        for x in 140..158 {
            let p = &got[(y * VIEW_W as usize + x) * 4..][..4];
            assert_eq!(
                p,
                [0, 0, 255, 255],
                "row {i} pixel ({x}, {y}) is {p:?}, not the red box"
            );
        }
    }
}

#[test]
fn a_badge_stays_over_its_image_and_an_image_over_its_text_whatever_is_off_screen() {
    let Some(gpu) = gpu() else {
        eprintln!("SKIP: no GPU adapter");
        return;
    };
    let ink = Color::new(0, 0, 0, 1.0);
    let badge = Color::new(255, 0, 0, 1.0);
    let list = vec![
        fill(
            0.0,
            0.0,
            VIEW_W as f32,
            4000.0,
            Color::new(255, 255, 255, 1.0),
        ),
        image("a.png", 100.0, 100.0, 200.0, 100.0),
        // Off screen at scroll 0: a fill over text, which flushed the batch
        // before, image and all, and so put the badge below on top.
        text(LINE, 100.0, 3000.0, ink),
        fill(100.0, 3000.0, 300.0, 20.0, Color::new(0, 200, 0, 1.0)),
        fill(150.0, 120.0, 60.0, 40.0, badge),
        // An image over earlier text.
        text("MMMMMMMMMMMMMMMMMMMM", 100.0, 300.0, ink),
        image("b.png", 120.0, 296.0, 60.0, 24.0),
    ];
    for scroll in [(0.0, 0.0), (0.0, 2600.0)] {
        let (want, _) = reference_draw(&gpu, &list, scroll);
        let (got, stats) = fast_draw(&gpu, &list, scroll);
        assert_eq!(want, got, "at {scroll:?}");
        if scroll.1 == 0.0 {
            assert!(stats.culled >= 2, "{stats:?}");
            let px = |x: usize, y: usize| got[(y * VIEW_W as usize + x) * 4..][..4].to_vec();
            // BGRA: the badge over the image, the image over the text.
            assert_eq!(px(180, 140), vec![0, 0, 255, 255], "badge under its image");
            assert_eq!(
                px(150, 308),
                vec![200, 120, 30, 255],
                "image under earlier text"
            );
        }
    }
}

#[test]
fn a_batch_past_the_buffer_limit_is_flushed_instead_of_panicking() {
    let Some(gpu) = gpu() else {
        eprintln!("SKIP: no GPU adapter");
        return;
    };
    // 80,000 rows of text drawn without culling: about 3.4M glyph quads,
    // past wgpu's 256 MB buffer limit in one batch. This panicked in
    // `flush_to` (Texture Vertex Buffer).
    let list = page(Mix::Text, 80_000);
    let mut r = renderer(&gpu);
    r.reference = true;
    let (_t, view) = screenshot::create_offscreen_target(&gpu.device, VIEW_W, VIEW_H, FORMAT);
    r.execute(&list, &view).expect("the frame draws");
    let s = r.last_frame_stats();
    assert!(s.batch_flushes >= 2, "{s:?}");
}

fn count_ink(px: &[u8]) -> usize {
    px.chunks_exact(4)
        .filter(|p| p[..3] != [255, 255, 255])
        .count()
}

#[test]
fn a_batch_past_the_buffer_limit_is_drawn_in_pieces() {
    let Some(gpu) = gpu() else {
        eprintln!("SKIP: no GPU adapter");
        return;
    };
    // Fills, then images (two textures, alternating, so the runs split
    // too), then a screenful of text: no command lands under one batched
    // before it, so the frame is one batch. With a 64 KiB buffer limit
    // each part of it is many times over.
    let mut list = vec![fill(
        0.0,
        0.0,
        1024.0,
        768.0,
        Color::new(255, 255, 255, 1.0),
    )];
    for i in 0..36 {
        let y = i as f32 * 21.0;
        list.push(fill(0.0, y, 1024.0, 1.0, Color::new(200, 200, 200, 1.0)));
        for j in 0..40 {
            list.push(fill(
                j as f32 * 25.0,
                y + 1.0,
                3.0,
                3.0,
                Color::new(0, 0, 255, 1.0),
            ));
        }
    }
    for i in 0..36 {
        let y = i as f32 * 21.0 + 4.0;
        list.push(image(["a.png", "b.png"][i % 2], 0.0, y, 16.0, 16.0));
        list.push(image(["b.png", "a.png"][i % 2], 1000.0, y, 16.0, 16.0));
    }
    for i in 0..36 {
        list.push(text(
            &LINE.repeat(2),
            20.0,
            i as f32 * 21.0 + 4.0,
            Color::new(0, 0, 0, 1.0),
        ));
    }
    let (want, _) = fast_draw(&gpu, &list, (0.0, 0.0));
    let mut r = renderer(&gpu);
    upload_images(&mut r);
    r.max_buffer_bytes = 64 * 1024;
    let got = draw(&gpu, &mut r, &list, (0.0, 0.0));
    let s = r.last_frame_stats();
    assert_eq!(s.batch_flushes, 1, "one batch, split only at draw: {s:?}");
    let tex = std::mem::size_of::<TextureVertex>();
    assert!(
        r.texture_vertices.len() * tex > 4 * 64 * 1024,
        "glyph batch over the limit"
    );
    assert!(r.color_vertices.len() * std::mem::size_of::<ColorVertex>() > 2 * 64 * 1024);
    assert!(count_ink(&want) > 10_000);
    let differ = want
        .chunks_exact(4)
        .zip(got.chunks_exact(4))
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(differ, 0, "{differ} pixels differ from the one-draw frame");

    // Images alone, more quads than one piece holds, to split inside runs.
    let mut r = renderer(&gpu);
    upload_images(&mut r);
    r.max_buffer_bytes = 4 * 1024;
    let tiles: Vec<_> = (0..1200)
        .map(|i| {
            let (x, y) = ((i % 40) as f32 * 25.0, (i / 40) as f32 * 25.0);
            image(["a.png", "b.png"][(i / 300) % 2], x, y, 20.0, 20.0)
        })
        .collect();
    let got = draw(&gpu, &mut r, &tiles, (0.0, 0.0));
    let (want, _) = fast_draw(&gpu, &tiles, (0.0, 0.0));
    assert!(r.image_vertices.len() * tex > 8 * 4 * 1024);
    assert_eq!(want, got);
}

#[test]
fn a_line_of_millions_of_characters_is_drawn() {
    let Some(gpu) = gpu() else {
        eprintln!("SKIP: no GPU adapter");
        return;
    };
    // One on-screen `Text` of 2.3 M characters (a minified bundle shown as
    // text/plain): one command, 2.3 M glyph quads, 294 MB of vertices,
    // past the 256 MB limit before any flush between commands can help.
    // It panicked in `flush_to`, then failed the frame with
    // `BufferTooLarge`; the draw now splits it.
    let line = "x".repeat(2_300_000);
    let list = vec![
        fill(0.0, 0.0, 1024.0, 768.0, Color::new(255, 255, 255, 1.0)),
        text(&line, 16.0, 100.0, Color::new(0, 0, 0, 1.0)),
    ];
    let mut r = renderer(&gpu);
    let px = draw(&gpu, &mut r, &list, (0.0, 0.0));
    assert!(
        (r.texture_vertices.len() * std::mem::size_of::<TextureVertex>()) as u64 > MAX_BUFFER_SIZE,
        "the batch is over the limit"
    );
    assert!(count_ink(&px) > 1_000, "the start of the line is drawn");
}

fn deco(x: f32, y: f32, width: f32, thickness: f32, color: Color) -> DisplayCommand {
    DisplayCommand::TextDecoration {
        x,
        y,
        width,
        thickness,
        color,
        style: rustkit_layout::TextDecorationStyleValue::Solid,
    }
}

fn spaced(s: &str, x: f32, y: f32, advance: f32, color: Color) -> DisplayCommand {
    let mut t = text(s, x, y, color);
    if let DisplayCommand::Text {
        advances: Some(a), ..
    } = &mut t
    {
        a.iter_mut().for_each(|a| *a = advance);
    }
    t
}

/// Lists whose culled frame used to differ from the unculled one: a
/// decoration's order against an earlier image hung on an unrelated
/// flush below the fold (R1 HOLD @ 9891715b, A); negative widths and a
/// border side wider than its box were culled while drawn (B and the
/// border note). Each is repeated down the page, so every scroll offset
/// culls a different part of it.
fn cull_hazards() -> Vec<DisplayCommand> {
    let ink = Color::new(0, 0, 0, 1.0);
    let red = Color::new(255, 0, 0, 1.0);
    let blue = Color::new(0, 0, 255, 1.0);
    let mut list = vec![fill(
        0.0,
        0.0,
        VIEW_W as f32,
        6000.0,
        Color::new(255, 255, 255, 1.0),
    )];
    let mut y = 0.0;
    while y < 5400.0 {
        list.extend([
            // A: an underlined link over an image, with a fill over text
            // further down in between: that fill's flush used to be what
            // put the underline over the image.
            image("a.png", 100.0, y + 100.0, 300.0, 600.0),
            text(LINE, 100.0, y + 900.0, ink),
            fill(100.0, y + 900.0, 300.0, 20.0, Color::new(0, 200, 0, 1.0)),
            text("Read more", 120.0, y + 420.0, ink),
            deco(120.0, y + 436.0, 63.0, 3.0, red),
            deco(120.0, y + 426.0, 63.0, 1.0, blue),
            // A decoration over an earlier text run, not its own.
            text("MMMMMMMMMMMMMMMMMMMMMMMM", 450.0, y + 200.0, ink),
            text("x", 450.0, y + 230.0, ink),
            deco(440.0, y + 205.0, 200.0, 4.0, red),
            // An orphan decoration (no text before it) over an image.
            image("b.png", 650.0, y + 300.0, 100.0, 100.0),
            deco(640.0, y + 340.0, 140.0, 6.0, blue),
            // B: text right of the viewport walking back on screen, its
            // underline drawn leftwards; a fill with a negative width.
            spaced("ABCDEFGHIJKLMNOPQRST", 1100.0, y + 160.0, -15.0, ink),
            deco(1100.0, y + 174.0, -300.0, 2.0, red),
            fill(1300.0, y + 500.0, -500.0, 40.0, Color::new(0, 120, 0, 1.0)),
            fill(500.0, y + 640.0, 120.0, -60.0, Color::new(120, 0, 120, 1.0)),
            // A border whose top side is wider than its box and reaches
            // 380px below it; a rounded one the same.
            DisplayCommand::Border {
                color: Color::new(200, 100, 0, 1.0),
                rect: Rect::new(100.0, y - 200.0, 300.0, 20.0),
                top: 400.0,
                right: 0.0,
                bottom: 0.0,
                left: 0.0,
            },
            DisplayCommand::RoundedBorder {
                rect: Rect::new(700.0, y - 300.0, 200.0, 30.0),
                widths: [420.0, 2.0, 2.0, 2.0],
                colors: [Color::new(0, 150, 150, 1.0); 4],
                radius: rustkit_layout::BorderRadius::default(),
            },
            // Glyphs and an image after all of it.
            text("on top", 300.0, y + 430.0, Color::new(255, 255, 255, 1.0)),
            image("b.png", 140.0, y + 560.0, 40.0, 40.0),
        ]);
        y += 1200.0;
    }
    list
}

fn draw_with(
    gpu: &Gpu,
    list: &[DisplayCommand],
    scroll: (f32, f32),
    reference: bool,
    batch_bytes: u64,
) -> (Vec<u8>, FrameStats) {
    let mut r = renderer(gpu);
    let mut px = |rgb: [u8; 3]| [rgb[2], rgb[1], rgb[0], 255].repeat(16 * 16);
    r.upload_image("a.png", 16, 16, &px([200, 120, 30])).unwrap();
    r.upload_image("b.png", 16, 16, &px([30, 160, 220])).unwrap();
    r.reference = reference;
    r.max_batch_bytes = batch_bytes;
    let frame = draw(gpu, &mut r, list, scroll);
    (frame, r.last_frame_stats())
}

#[test]
fn culling_never_changes_a_pixel_of_decorations_negative_sizes_or_wide_borders() {
    let Some(gpu) = gpu() else {
        eprintln!("SKIP: no GPU adapter");
        return;
    };
    let hazards = cull_hazards();
    // The same list through the blur path, whose only flushes besides the
    // blur are the size cap's.
    let mut blurred = hazards.clone();
    blurred.push(DisplayCommand::BackdropFilter {
        rect: Rect::new(900.0, 700.0, 100.0, 60.0),
        border_radius: rustkit_layout::BorderRadius::default(),
        filter: rustkit_css::BackdropFilter::Blur(4.0),
    });
    let cases: [(&str, &[DisplayCommand], u64); 4] = [
        ("fast", &hazards, MAX_BATCH_BYTES),
        // A cap of 2 KiB flushes every few commands, at places that move
        // with culling.
        ("fast, small cap", &hazards, 2 * 1024),
        ("blur", &blurred, MAX_BATCH_BYTES),
        ("blur, small cap", &blurred, 2 * 1024),
    ];
    for (name, list, cap) in cases {
        for scroll in [
            (0.0, 0.0),
            (0.0, 150.0),
            (0.0, 300.0),
            (0.0, 1200.0),
            (7.0, 2111.5),
            (0.0, -40.0),
            (0.0, 4500.0),
        ] {
            let (want, _) = draw_with(&gpu, list, scroll, true, cap);
            let (got, stats) = draw_with(&gpu, list, scroll, false, cap);
            let differ = want
                .chunks_exact(4)
                .zip(got.chunks_exact(4))
                .filter(|(a, b)| a != b)
                .count();
            assert_eq!(differ, 0, "{name} at {scroll:?}: {differ} pixels differ");
            assert!(stats.culled > 0, "{name} at {scroll:?}: nothing culled");
        }
    }
    // What the reference draws is what CSS order says, at every scroll: the
    // underline over the image under it (BGRA red), the decoration over the
    // earlier run, the leftward underline and fill, and the wide border.
    let px = |f: &[u8], x: usize, y: usize| f[(y * VIEW_W as usize + x) * 4..][..4].to_vec();
    let red = vec![0, 0, 255, 255];
    for scroll_y in [0.0f32, 150.0] {
        let (got, _) = draw_with(&gpu, &hazards, (0.0, scroll_y), false, MAX_BATCH_BYTES);
        let at = |x: usize, y: f32| px(&got, x, (y - scroll_y) as usize);
        assert_eq!(at(170, 437.0), red, "underline at scroll {scroll_y}");
        assert_eq!(at(600, 206.0), red, "decoration over earlier text");
        if scroll_y > 0.0 {
            // At scroll 0 no transform is set and the negative sizes draw
            // nothing (`push_color_quad`), on both paths.
            assert_eq!(at(900, 175.0), red, "leftward underline");
            assert_eq!(at(900, 520.0), vec![0, 120, 0, 255], "leftward fill");
        }
    }
    let (got, _) = draw_with(&gpu, &hazards, (0.0, 0.0), false, MAX_BATCH_BYTES);
    assert_eq!(px(&got, 200, 150), vec![0, 100, 200, 255], "wide top border");
}

fn underline(x: f32, y: f32, width: f32, color: Color) -> DisplayCommand {
    DisplayCommand::TextDecoration {
        x,
        y,
        width,
        thickness: 3.0,
        color,
        style: rustkit_layout::TextDecorationStyleValue::Solid,
    }
}

#[test]
fn an_underline_over_an_earlier_image_and_negative_widths_draw_the_same_at_any_scroll() {
    let Some(gpu) = gpu() else {
        eprintln!("SKIP: no GPU adapter");
        return;
    };
    let ink = Color::new(0, 0, 0, 1.0);
    let list = vec![
        fill(
            0.0,
            0.0,
            VIEW_W as f32,
            4000.0,
            Color::new(255, 255, 255, 1.0),
        ),
        image("a.png", 100.0, 100.0, 200.0, 100.0),
        // Off screen at scroll 0: a fill over text, which flushed the batch
        // (image included) before the underlined text below.
        text(LINE, 100.0, 3000.0, ink),
        fill(100.0, 3000.0, 300.0, 20.0, Color::new(0, 200, 0, 1.0)),
        // Underlined text over the image: the underline lands on the image.
        text("underlined", 110.0, 150.0, ink),
        underline(110.0, 170.0, 150.0, Color::new(255, 0, 255, 1.0)),
        // Negative widths reach back left of `x` and up from `y`.
        fill(1100.0, 400.0, -300.0, 20.0, Color::new(0, 160, 0, 1.0)),
        underline(1100.0, 440.0, -300.0, Color::new(0, 0, 200, 1.0)),
    ];
    for scroll in [(0.0, 0.0), (0.0, 2600.0), (40.0, 0.0)] {
        let (want, _) = reference_draw(&gpu, &list, scroll);
        let (got, stats) = fast_draw(&gpu, &list, scroll);
        let differ = want
            .chunks_exact(4)
            .zip(got.chunks_exact(4))
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(differ, 0, "at {scroll:?}: {differ} pixels differ");
        let px = |x: usize, y: usize| got[(y * VIEW_W as usize + x) * 4..][..4].to_vec();
        if scroll == (0.0, 0.0) {
            assert!(stats.culled >= 2, "{stats:?}");
            // BGRA: the underline over the image.
            assert_eq!(
                px(250, 171),
                vec![255, 0, 255, 255],
                "underline under the image"
            );
        }
        if scroll == (40.0, 0.0) {
            // Under the scroll translate the negative-width bars are drawn
            // (unscrolled, the renderer drops a negative-size quad), 40px
            // left of where they are laid out.
            assert_eq!(
                px(860, 410),
                vec![0, 160, 0, 255],
                "negative-width fill missing"
            );
            assert_eq!(
                px(860, 441),
                vec![200, 0, 0, 255],
                "negative-width underline missing"
            );
        }
    }
}
