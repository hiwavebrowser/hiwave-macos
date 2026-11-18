use rustkit_codecs::decode_png;

fn encode_rgba(width: u32, height: u32) -> (Vec<u8>, Vec<u8>) {
    let pixels: Vec<_> = (0..width * height)
        .flat_map(|p| [p as u8, (p / width) as u8, 64, (p >> 3) as u8])
        .collect();
    let mut encoded = Vec::new();
    let mut encoder = png::Encoder::new(&mut encoded, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&pixels)
        .unwrap();
    (encoded, pixels)
}

#[test]
fn rgba_png_preserves_dimensions_and_pixel_bytes() {
    let (bytes, expected) = encode_rgba(37, 23);
    let image = decode_png(&bytes).unwrap();
    assert_eq!((image.width(), image.height()), (37, 23));
    assert_eq!(image.data(), expected);
}

#[test]
#[ignore = "release-mode PNG decode timing"]
fn rgba_png_decode_timing() {
    let (bytes, _) = encode_rgba(1920, 1080);
    let mut samples = Vec::new();
    for _ in 0..21 {
        let start = std::time::Instant::now();
        let image = decode_png(std::hint::black_box(&bytes)).unwrap();
        samples.push(start.elapsed());
        std::hint::black_box(image);
    }
    samples.sort_unstable();
    eprintln!("1080p RGBA decode median {:?}", samples[samples.len() / 2]);
}
