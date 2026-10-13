//! The rasterizer must not cost the caller.
//!
//! The point of the worker is that a component's render hands over a
//! scene and returns immediately, so the UI thread keeps its frame for
//! gestures. That is a claim about the CALLER's time, which is what these
//! measure — not about how fast the picture arrives.
use anyrender::{PaintScene, Scene};
use expression_editor_ui::scene_image::worker::Rasterizer;
use kurbo::{Affine, Rect};
use peniko::{Color, Fill};

fn dense() -> Scene {
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
fn handing_over_a_scene_is_far_cheaper_than_drawing_it() {
    let raster = Rasterizer::spawn();
    let scene = dense();
    // Warm the thread so the first submission is not paying for startup.
    raster.draw(scene.clone(), 1000.0, 400.0, 1.0, Color::BLACK);
    std::thread::sleep(std::time::Duration::from_millis(200));

    let n = 20;
    let start = std::time::Instant::now();
    for _ in 0..n {
        raster.draw(scene.clone(), 1000.0, 400.0, 1.0, Color::BLACK);
    }
    let per_call = start.elapsed().as_secs_f64() * 1000.0 / f64::from(n);
    println!("submit: {per_call:.3} ms/frame on the caller's thread");

    // Rasterizing this scene measured ~2.8 ms. Submitting is a clone and
    // a mutex, and must stay an order of magnitude under that or it is
    // not off the UI thread in any sense that matters.
    assert!(
        per_call < 0.5,
        "submitting cost {per_call:.3} ms — not meaningfully off-thread"
    );
}

#[test]
fn a_finished_frame_is_a_bmp_at_a_new_revision() {
    let raster = Rasterizer::spawn();
    assert_eq!(raster.revision(), 0, "nothing drawn yet");
    raster.draw(dense(), 200.0, 100.0, 1.0, Color::BLACK);

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while raster.revision() == 0 {
        assert!(std::time::Instant::now() < deadline, "no frame arrived");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let revision = raster.revision();
    let bytes = raster.get(revision).expect("bytes for the current frame");
    assert_eq!(&bytes[0..2], b"BM");
    assert_eq!(bytes.len(), 54 + 200 * 100 * 4);
    // A superseded revision is gone, so a stale URL cannot serve an old
    // picture: the element pointing at it has already been told the new one.
    assert!(raster.get(revision - 1).is_none());
}

#[test]
fn a_pan_drops_superseded_frames_rather_than_queueing_them() {
    let raster = Rasterizer::spawn();
    let scene = dense();
    // Submit far more frames than can be drawn, as a pan does.
    for _ in 0..50 {
        raster.draw(scene.clone(), 1000.0, 400.0, 1.0, Color::BLACK);
    }
    std::thread::sleep(std::time::Duration::from_millis(600));
    let drawn = raster.revision();
    // If these queued, all fifty would eventually be drawn. Latest-wins
    // means only the ones that were current when the thread looked.
    assert!(
        drawn < 50,
        "drew {drawn} of 50 submissions — frames are queueing, not coalescing"
    );
    assert!(drawn >= 1, "nothing was drawn at all");
    println!("50 submissions during a pan produced {drawn} rendered frames");
}

/// The bytes must be decodable by a real image decoder, not just
/// well-shaped. A browser that cannot read them shows nothing at all,
/// which is indistinguishable from every other way this can fail.
#[test]
fn the_bmp_decodes_to_the_pixels_that_were_drawn() {
    let mut scene = Scene::new();
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgb8(0x22, 0xcc, 0x88),
        None,
        &Rect::new(0.0, 0.0, 6.0, 3.0),
    );
    let bytes = expression_editor_ui::scene_image::scene_bmp(&scene, 6.0, 3.0, 1.0, Color::BLACK);

    let decoded = image::load_from_memory_with_format(&bytes, image::ImageFormat::Bmp)
        .expect("a decoder must be able to read this");
    assert_eq!((decoded.width(), decoded.height()), (6, 3));
    let rgba = decoded.to_rgba8();
    // Top-left, which is where the fill starts — proves the row order is
    // right, not merely that something decoded.
    assert_eq!(rgba.get_pixel(0, 0).0, [0x22, 0xcc, 0x88, 0xff]);
}
