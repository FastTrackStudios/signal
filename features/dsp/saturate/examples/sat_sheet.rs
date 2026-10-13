//! Every saturation picture, rendered on the CPU, to PNGs.
//!
//!     cargo run -p saturate-ui --features viz --example sat_sheet -- out/
//!
//! Nine profiles at two drives, one with a live level, and the multiband
//! overview — each reachable without a window, a GPU or a host, so the
//! circuits nobody happens to load still get looked at.

use std::error::Error;
use std::fs::File;
use std::io::BufWriter;

use anyrender::{ImageRenderer, PaintScene, Scene};
use anyrender_vello_cpu::VelloCpuImageRenderer;
use kurbo::{Affine, Rect};
use peniko::{Color, Fill};
use saturate::ui::viz::{BandView, MultibandSatView, SatView, paint_multiband_sat, paint_sat};

const W: u32 = 760;
const H: u32 = 190;

fn main() -> Result<(), Box<dyn Error>> {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/sat-viz".into());
    std::fs::create_dir_all(&dir)?;

    for profile in saturate::profiles::PROFILES {
        let base = SatView::of(*profile, saturate::profiles::Controls::default());
        for (tag, drive, level) in [
            ("lo", 0.2, None),
            ("hi", 0.75, None),
            ("live", 0.5, Some(0.6)),
        ] {
            let mut view = base;
            view.controls.drive = drive;
            view.level = level;
            let mut scene = Scene::new();
            paint_sat(&mut scene, &view, f64::from(W), f64::from(H));
            let name = format!("{dir}/sat-{}-{tag}.png", profile.id);
            write_png(&name, &render(scene, W, H), W, H)?;
            println!("{name}");
        }
    }

    let (mw, mh) = (900_u32, 260_u32);
    for (tag, view) in [("four", four_bands()), ("muted", muted())] {
        let mut scene = Scene::new();
        paint_multiband_sat(&mut scene, &view, f64::from(mw), f64::from(mh));
        let name = format!("{dir}/sat-multiband-{tag}.png");
        write_png(&name, &render(scene, mw, mh), mw, mh)?;
        println!("{name}");
    }
    Ok(())
}

fn band(id: &str, drive: f32) -> Option<BandView> {
    let mut band = BandView::new(id)?;
    band.controls.drive = drive;
    Some(band)
}

fn four_bands() -> MultibandSatView {
    let mut bands: Vec<BandView> = [
        band("transformer", 0.7),
        band("triode", 0.45),
        band("tape_hot", 0.6),
        band("crush", 0.3),
    ]
    .into_iter()
    .flatten()
    .collect();
    if let Some(bass) = bands.first_mut() {
        // A falling spectrum, so the drawn line has something to say.
        bass.spectrum = (0..96_u8)
            .map(|i| f32::from(i).mul_add(-0.6, -18.0))
            .collect();
    }
    MultibandSatView {
        crossovers: vec![160.0, 1_400.0, 6_500.0],
        bands,
        selected: Some(1),
        on: true,
    }
}

fn muted() -> MultibandSatView {
    let mut view = four_bands();
    view.crossovers = vec![90.0, 600.0, 3_000.0, 9_000.0];
    view.bands.extend(band("fuzz", 0.9));
    if let Some(b) = view.bands.get_mut(1) {
        b.mute = true;
    }
    if let Some(b) = view.bands.get_mut(4) {
        b.solo = true;
    }
    if let Some(b) = view.bands.get_mut(2) {
        b.solo = true;
        b.level_db = 4.0;
    }
    view.selected = Some(4);
    view
}

/// The scene over the rack's own ground, so the PNG looks like the panel.
fn render(scene: Scene, w: u32, h: u32) -> Vec<u8> {
    let mut renderer = VelloCpuImageRenderer::new(w, h);
    let mut buf = Vec::new();
    renderer.render_to_vec(
        |painter| {
            painter.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                Color::from_rgba8(12, 14, 18, 255),
                None,
                &Rect::new(0.0, 0.0, f64::from(w), f64::from(h)),
            );
            painter.append_scene(scene, Affine::IDENTITY);
        },
        &mut buf,
    );
    buf
}

fn write_png(path: &str, rgba: &[u8], w: u32, h: u32) -> Result<(), Box<dyn Error>> {
    let mut enc = png::Encoder::new(BufWriter::new(File::create(path)?), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?.write_image_data(rgba)?;
    Ok(())
}
