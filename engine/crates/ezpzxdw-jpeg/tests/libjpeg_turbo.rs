//! Every `tests/fixtures/*.jpg` must decode to exactly its `*.png`, which is libjpeg-turbo's
//! decode with the fast integer IDCT and fancy upsampling (4:4:0: replication). See
//! `tests/fixtures/README.md` for how the files were made.

use std::path::{Path, PathBuf};

use ezpzxdw_jpeg::{Decoder, PixelFormat};

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// The answer: width, height, channels (1 or 3), samples.
fn answer(path: &Path) -> (u32, u32, usize, Vec<u8>) {
    let bytes = std::fs::read(path).unwrap();
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!(info.bit_depth, png::BitDepth::Eight, "{}: bit depth", path.display());
    let (w, h) = (info.width, info.height);
    let (channels, stride) = match info.color_type {
        png::ColorType::Grayscale => (1, 1),
        png::ColorType::Rgb => (3, 3),
        png::ColorType::Rgba => (3, 4),
        other => panic!("{}: colour type {other:?}", path.display()),
    };
    let mut out = Vec::with_capacity((w * h) as usize * channels);
    for y in 0..h as usize {
        let row = &buf[y * info.line_size..];
        for x in 0..w as usize {
            out.extend_from_slice(&row[x * stride..x * stride + channels]);
        }
    }
    (w, h, channels, out)
}

#[test]
fn decodes_like_libjpeg_turbo_ifast_fancy() {
    let mut jpgs: Vec<PathBuf> = std::fs::read_dir(dir())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "jpg"))
        .collect();
    jpgs.sort();
    // make.py writes 104 cases; a missing or extra file means the fixtures are out of step.
    assert_eq!(jpgs.len(), 104, "number of fixture cases");

    let mut failures = Vec::new();
    for jpg in &jpgs {
        let name = jpg.file_stem().unwrap().to_string_lossy().into_owned();
        let (w, h, channels, expected) = answer(&jpg.with_extension("png"));

        let mut decoder = Decoder::new(std::fs::File::open(jpg).unwrap());
        let pixels = decoder.decode().unwrap_or_else(|e| panic!("{name}: {e}"));
        let info = decoder.info().unwrap();
        assert_eq!((u32::from(info.width), u32::from(info.height)), (w, h), "{name}: size");
        let got_channels = match info.pixel_format {
            PixelFormat::L8 => 1,
            PixelFormat::RGB24 => 3,
            other => panic!("{name}: pixel format {other:?}"),
        };
        assert_eq!(got_channels, channels, "{name}: channels");
        assert_eq!(pixels.len(), expected.len(), "{name}: sample count");

        let mut max = 0u8;
        let mut at = (0, 0, 0);
        let mut count = 0usize;
        for (i, (a, b)) in pixels.iter().zip(&expected).enumerate() {
            let d = a.abs_diff(*b);
            if d > 0 {
                count += 1;
            }
            if d > max {
                max = d;
                let p = i / channels;
                at = (p % w as usize, p / w as usize, i % channels);
            }
        }
        if max > 0 {
            failures.push(format!(
                "{name}: max difference {max} at x={} y={} channel {} ({count} samples differ)",
                at.0, at.1, at.2
            ));
        }
    }
    assert!(failures.is_empty(), "{} of {} cases differ:\n{}", failures.len(), jpgs.len(), failures.join("\n"));
    println!("{} cases, all difference 0", jpgs.len());
}
