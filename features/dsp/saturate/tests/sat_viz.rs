//! The saturation pictures, rasterised: each draws something, no two
//! circuits draw the same thing, and drive moves the pixels.
//!
//! Rendered on the CPU through `anyrender_vello_cpu`, the same replay a GPU
//! backend does, so this looks at the picture rather than at a command list
//! that might rasterise to nothing.

use anyrender::{ImageRenderer, PaintScene, Scene};
use anyrender_vello_cpu::VelloCpuImageRenderer;
use dsp_core::num;
use kurbo::{Affine, Rect};
use peniko::{Color, Fill};
use saturate::ui::viz::{BandView, MultibandSatView, SatView, paint_multiband_sat, paint_sat};

const W: u32 = 480;
const H: u32 = 140;
const GROUND: [u8; 3] = [12, 14, 18];

fn rasterise(scene: Scene, w: u32, h: u32) -> Vec<u8> {
    let mut renderer = VelloCpuImageRenderer::new(w, h);
    let mut buf = Vec::new();
    renderer.render_to_vec(
        |painter| {
            painter.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                Color::from_rgba8(GROUND[0], GROUND[1], GROUND[2], 255),
                None,
                &Rect::new(0.0, 0.0, f64::from(w), f64::from(h)),
            );
            painter.append_scene(scene, Affine::IDENTITY);
        },
        &mut buf,
    );
    buf
}

fn sat(id: &str, drive: f32) -> Vec<u8> {
    let Some(mut view) = SatView::new(id) else {
        return Vec::new();
    };
    view.controls.drive = drive;
    let mut scene = Scene::new();
    paint_sat(&mut scene, &view, f64::from(W), f64::from(H));
    rasterise(scene, W, H)
}

/// Share of pixels that moved off the ground by a visible amount.
fn coverage(rgba: &[u8]) -> f64 {
    let lit = rgba
        .chunks_exact(4)
        .filter(|px| {
            px.iter()
                .take(3)
                .zip(GROUND)
                .any(|(&c, g)| c.abs_diff(g) > 24)
        })
        .count();
    num::count_to_f64(lit) / num::count_to_f64(rgba.len() / 4).max(1.0)
}

/// Mean per-channel difference between two frames, 0..=1.
fn difference(a: &[u8], b: &[u8]) -> f64 {
    let total: u64 = a
        .iter()
        .zip(b)
        .map(|(&x, &y)| u64::from(x.abs_diff(y)))
        .sum();
    num::u64_to_f64(total) / (num::count_to_f64(a.len()) * 255.0).max(1.0)
}

const ONE_PER_CIRCUIT: [&str; 5] = ["triode", "tape", "transformer", "transistor", "crush"];

#[test]
fn every_circuit_draws_something_and_no_two_draw_the_same_thing() {
    let frames: Vec<_> = ONE_PER_CIRCUIT
        .iter()
        .map(|id| (*id, sat(id, 0.6)))
        .collect();
    for (id, frame) in &frames {
        let c = coverage(frame);
        assert!(c > 0.03, "{id} drew almost nothing (coverage {c:.3})");
    }
    for (i, (a, fa)) in frames.iter().enumerate() {
        for (b, fb) in &frames[i + 1..] {
            let d = difference(fa, fb);
            assert!(
                d > 0.01,
                "{a} and {b} draw the same picture (difference {d:.4})"
            );
        }
    }
}

#[test]
fn drive_moves_the_pixels_on_every_profile() {
    for profile in saturate::profiles::PROFILES {
        let d = difference(&sat(profile.id, 0.1), &sat(profile.id, 0.9));
        assert!(
            d > 0.001,
            "{}: drive barely moved the picture ({d:.5})",
            profile.id
        );
    }
}

#[test]
fn the_multiband_view_draws_and_moves() {
    let bands: Vec<BandView> = ["transformer", "triode", "tape", "crush"]
        .iter()
        .map(|id| BandView::new(id).unwrap())
        .collect();
    let view = MultibandSatView {
        crossovers: vec![180.0, 1_500.0, 6_000.0],
        bands,
        selected: Some(1),
        on: true,
    };
    let draw = |v: &MultibandSatView| {
        let mut scene = Scene::new();
        paint_multiband_sat(&mut scene, v, 900.0, 260.0);
        rasterise(scene, 900, 260)
    };
    let reference = draw(&view);
    assert!(coverage(&reference) > 0.05);

    let mut driven = view.clone();
    driven.bands[0].controls.drive = 1.0;
    assert!(difference(&reference, &draw(&driven)) > 0.001, "drive");

    let mut moved = view;
    moved.crossovers[0] = 400.0;
    assert!(difference(&reference, &draw(&moved)) > 0.001, "crossover");
}
