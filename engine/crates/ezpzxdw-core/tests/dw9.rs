//! DocuWorks 9 samples (tests/fixtures/dw9, see its README): the engine
//! decodes each picture the way DocuWorks 9 draws it.

use std::path::PathBuf;

use ezpzxdw_core::doc::Document;
use ezpzxdw_core::gfx::ImageData;

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dw9")
}

/// DocuWorks' own drawing of the picture: width, height, RGB bytes.
fn answer(name: &str) -> (u32, u32, Vec<u8>) {
    let bytes = std::fs::read(dir().join(format!("{name}.png"))).unwrap();
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!(info.bit_depth, png::BitDepth::Eight, "{name}.png: bit depth");
    let (w, h) = (info.width, info.height);
    let px = (w * h) as usize;
    let mut rgb = Vec::with_capacity(px * 3);
    for y in 0..h as usize {
        let row = &buf[y * info.line_size..];
        for x in 0..w as usize {
            match info.color_type {
                png::ColorType::Rgb => rgb.extend_from_slice(&row[x * 3..x * 3 + 3]),
                png::ColorType::Rgba => rgb.extend_from_slice(&row[x * 4..x * 4 + 3]),
                png::ColorType::Grayscale => rgb.extend_from_slice(&[row[x]; 3]),
                png::ColorType::GrayscaleAlpha => rgb.extend_from_slice(&[row[x * 2]; 3]),
                other => panic!("{name}.png: color type {other:?}"),
            }
        }
    }
    (w, h, rgb)
}

#[test]
fn pictures_match_docuworks_9() {
    let cases: [(&str, u8, f64); 7] = [
        ("paste-grad-64", 0, 0.0),
        ("paste-odd-67x66", 0, 0.0),
        ("api-odd-67x66", 0, 0.0),
        ("file-line-300x200", 5, 0.5),
        ("mrc-two-300x200", 5, 0.5),
        ("mrc-grey-401x303", 1, 0.5),
        ("api-drawing-800x600", 0, 0.0),
    ];
    for (name, max, mean) in cases {
        let bytes = std::fs::read(dir().join(format!("{name}.xdw"))).unwrap();
        let doc = Document::open(bytes).unwrap();
        let display = doc.render(0).unwrap();
        assert!(display.skipped.is_empty(), "{name}: skipped {:?}", display.skipped);
        assert_eq!(display.objects_not_drawn, 0, "{name}: objects not drawn");
        assert_eq!(display.images.len(), 1, "{name}: images");
        let image = &display.images[0];
        let ImageData::Rgba(rgba) = &image.data else {
            panic!("{name}: the picture is not RGBA");
        };
        let (w, h, rgb) = answer(name);
        assert_eq!((image.w, image.h), (w, h), "{name}: size");
        assert_eq!(rgba.len(), (w * h * 4) as usize, "{name}: RGBA length");

        // DocuWorks leaves the page's first row and column white: skip them.
        let (mut largest, mut sum, mut count) = (0u8, 0u64, 0u64);
        for y in 1..h as usize {
            for x in 1..w as usize {
                let i = y * w as usize + x;
                for c in 0..3 {
                    let d = rgba[i * 4 + c].abs_diff(rgb[i * 3 + c]);
                    largest = largest.max(d);
                    sum += d as u64;
                    count += 1;
                }
            }
        }
        let avg = sum as f64 / count as f64;
        println!("{name}: {w}x{h}, largest difference {largest}, mean {avg:.4}");
        assert!(
            largest <= max && avg <= mean,
            "{name}: largest difference {largest} (allowed {max}), mean {avg:.4} (allowed {mean})"
        );
    }
}
