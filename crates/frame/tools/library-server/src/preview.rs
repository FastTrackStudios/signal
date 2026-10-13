//! Thumbnails: a surface's design rendered offscreen to a small PNG.

use std::sync::{Arc, Mutex};

use frame::document::proto::{PaintKind, Scene};
use frame::library::fs::Previewer;
use frame::render::offscreen::Offscreen;

/// Thumbnail width, px.
const WIDTH: f64 = 480.0;
/// Background under a design with no solid fill.
const DARK: peniko::Color = peniko::Color::from_rgb8(0x0e, 0x0f, 0x12);

/// A GPU previewer, or `None` (with a note) when there's no GPU.
pub fn previewer() -> Option<Previewer> {
    match Offscreen::new() {
        Ok(gpu) => {
            let gpu = Arc::new(Mutex::new(gpu));
            Some(Arc::new(move |scene: &Scene| {
                let mut gpu = gpu
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                match thumbnail(&mut gpu, scene) {
                    Ok(png) => Some(png),
                    Err(e) => {
                        eprintln!("frame-library-server: preview failed: {e}");
                        None
                    }
                }
            }))
        }
        Err(e) => {
            eprintln!("frame-library-server: no GPU for previews ({e}); serving without them");
            None
        }
    }
}

/// The first page's first top-level node, scaled to [`WIDTH`], as PNG.
fn thumbnail(gpu: &mut Offscreen, scene: &Scene) -> Result<Vec<u8>, String> {
    let page = *scene.pages().first().ok_or("no page")?;
    let boxes = frame::render::build_scene_layout_boxes(scene, page);
    let top = scene
        .node(page)
        .and_then(|p| p.children.first().copied())
        .ok_or("empty page")?;
    let b = boxes.get(&top).ok_or("no layout for the top node")?;
    if b.width <= 0.0 || b.height <= 0.0 {
        return Err("empty design".into());
    }
    let scale = WIDTH / b.width;
    let (w, h) = (
        (b.width * scale).ceil() as u32,
        (b.height * scale).ceil().min(WIDTH * 2.0) as u32,
    );
    let base = scene
        .node(top)
        .and_then(|n| n.fills.iter().rev().find(|p| p.visible))
        .and_then(|p| match p.kind {
            PaintKind::Solid { color: c } => {
                let u8 = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
                Some(peniko::Color::from_rgb8(u8(c.r), u8(c.g), u8(c.b)))
            }
            _ => None,
        })
        .unwrap_or(DARK);
    let px = gpu.render_with_effects(&frame::render::fx::Frame {
        transform: kurbo::Affine::scale(scale) * kurbo::Affine::translate((-b.x, -b.y)),
        base,
        fonts: Some(frame::render::FontBook::bundled()),
        ..frame::render::fx::Frame::new(scene, page, w, h)
    })?;
    let img = image::RgbaImage::from_raw(px.width, px.height, px.rgba).ok_or("bad pixel buffer")?;
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}
