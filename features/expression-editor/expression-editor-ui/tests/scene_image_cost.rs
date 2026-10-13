//! What a painted pane costs to present as an image.
use anyrender::{PaintScene, Scene};
use expression_editor_ui::scene_image::{scene_bmp, scene_data_uri};
use kurbo::{Affine, Rect};
use peniko::{Color, Fill};

/// PNG data URI (portable/web path) against BMP bytes (desktop path).
#[test]
#[ignore = "measurement"]
fn png_uri_versus_served_bmp() {
    let scene = dense_scene();
    for (w, h, scale) in [(1000.0, 400.0, 1.0), (1000.0, 400.0, 2.0)] {
        let n = 10;

        let t = std::time::Instant::now();
        let mut uri_len = 0;
        for _ in 0..n {
            uri_len = scene_data_uri(&scene, w, h, scale, Color::BLACK).len();
        }
        let png_ms = t.elapsed().as_secs_f64() * 1000.0 / f64::from(n);

        let t = std::time::Instant::now();
        let mut bmp_len = 0;
        for _ in 0..n {
            bmp_len = scene_bmp(&scene, w, h, scale, Color::BLACK).len();
        }
        let bmp_ms = t.elapsed().as_secs_f64() * 1000.0 / f64::from(n);

        println!(
            "{w:.0}x{h:.0} scale {scale}:  png+base64 {png_ms:6.2} ms ({:.2} MB through the DOM)\
             \n                        bmp served {bmp_ms:6.2} ms ({:.2} MB, never in the DOM)   \
             {:.0}% faster",
            uri_len as f64 / 1e6,
            bmp_len as f64 / 1e6,
            (png_ms - bmp_ms) / png_ms * 100.0
        );
    }
}

/// Where the milliseconds actually go: rasterize, encode, base64.
#[test]
#[ignore = "measurement"]
fn cost_breakdown() {
    use anyrender::render_to_buffer;
    use anyrender_vello_cpu::VelloCpuImageRenderer;
    let scene = dense_scene();
    for (w, h, scale) in [(1000u32, 400u32, 1u32), (1000, 400, 2)] {
        let (pw, ph) = (w * scale, h * scale);
        let n = 10;

        let t = std::time::Instant::now();
        let mut buffer = Vec::new();
        for _ in 0..n {
            buffer = render_to_buffer::<VelloCpuImageRenderer, _>(
                |painter: &mut <VelloCpuImageRenderer as anyrender::ImageRenderer>::ScenePainter<'_>| {
                    painter.append_scene(scene.clone(), Affine::scale(f64::from(scale)));
                },
                pw,
                ph,
            );
        }
        let raster = t.elapsed().as_secs_f64() * 1000.0 / f64::from(n);

        let t = std::time::Instant::now();
        let mut png = Vec::new();
        for _ in 0..n {
            png.clear();
            let mut enc = png::Encoder::new(&mut png, pw, ph);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            enc.set_compression(png::Compression::Fast);
            let mut wr = enc.write_header().unwrap();
            wr.write_image_data(&buffer).unwrap();
        }
        let encode = t.elapsed().as_secs_f64() * 1000.0 / f64::from(n);

        let t = std::time::Instant::now();
        for _ in 0..n {
            use base64::Engine as _;
            let mut uri = String::new();
            base64::engine::general_purpose::STANDARD.encode_string(&png, &mut uri);
        }
        let b64 = t.elapsed().as_secs_f64() * 1000.0 / f64::from(n);

        println!(
            "{pw}x{ph} (scale {scale}): raster {raster:6.2}  png {encode:6.2}  base64 {b64:6.2}  \
             = {:6.2} ms   [raw RGBA would be {:.2} MB]",
            raster + encode + b64,
            buffer.len() as f64 / 1e6
        );
    }
}

fn dense_scene() -> Scene {
    let mut scene = Scene::new();
    for i in 0..3000 {
        let x = (i % 600) as f64 * 2.6;
        let y = (i / 600) as f64 * 80.0;
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgb8(0x22, 0xcc, 0x88),
            None,
            &Rect::new(x, y, x + 2.0, y + 60.0),
        );
    }
    scene
}

#[test]
#[ignore = "measurement"]
fn cost_at_pane_size() {
    // A stand-in for the drum stack's real load: a few thousand small
    // shapes, which is what its lanes of hits and grid amount to.
    let mut scene = Scene::new();
    for i in 0..3000 {
        let x = (i % 600) as f64 * 2.6;
        let y = (i / 600) as f64 * 80.0;
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgb8(0x22, 0xcc, 0x88),
            None,
            &Rect::new(x, y, x + 2.0, y + 60.0),
        );
    }
    for (w, h, label) in [(1000.0, 400.0, "stack"), (1600.0, 900.0, "full window")] {
        for scale in [1.0, 2.0] {
            let t = std::time::Instant::now();
            let n = 10;
            let mut bytes = 0;
            for _ in 0..n {
                let uri = scene_data_uri(&scene, w, h, scale, Color::BLACK);
                bytes = uri.len();
            }
            let ms = t.elapsed().as_secs_f64() * 1000.0 / f64::from(n);
            println!(
                "{label:12} {w:>6.0}x{h:<5.0} scale {scale}  {ms:7.2} ms/frame  \
                 {:.1} fps  uri {:.2} MB",
                1000.0 / ms,
                bytes as f64 / 1e6
            );
        }
    }
}
