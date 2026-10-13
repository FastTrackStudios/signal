//! Two machines must not draw the same picture.
//!
//! The per-family shader branches are the kind of thing that rots quietly: a
//! constant drifts, a branch stops being reached, and the panel goes back to
//! drawing every algorithm as the same wedge — which is exactly the state
//! this work started from. Nothing else would notice, because every frame is
//! still a legal picture.
//!
//! Rendered on a real device through `fts_audio_ui::shader::probe`, because
//! the branches are in WGSL and a Rust-side assertion would be testing a
//! different program.

use reverb_ui::viz::{Family, ReverbUniforms, ReverbView, SHADER};

const W: u32 = 480;
const H: u32 = 140;

fn view(family: Family) -> ReverbView {
    ReverbView {
        decay: 2.6,
        density: 0.55,
        predelay: 0.035,
        mix: 0.5,
        damp: 0.35,
        family,
        on: true,
        beat: 0.5,
        color: [167, 139, 250],
        time: 1.4,
    }
}

#[test]
fn every_family_draws_something_and_no_two_draw_the_same_thing() {
    let Some(probe) = fts_audio_ui::shader::probe::Probe::open() else {
        eprintln!("no GPU here — skipping");
        return;
    };

    let frames: Vec<_> = Family::ALL
        .iter()
        .map(|&f| {
            let u = ReverbUniforms::of(&view(f), W as f32, H as f32);
            (f, probe.render(SHADER, bytemuck::bytes_of(&u), W, H))
        })
        .collect();

    for (family, frame) in &frames {
        assert!(
            frame.coverage() > 0.02,
            "{family:?} drew nothing — a branch that renders an empty panel is \
             indistinguishable from a shader that failed"
        );
    }

    for (i, (a, fa)) in frames.iter().enumerate() {
        for (b, fb) in &frames[i + 1..] {
            let d = fa.difference(fb);
            assert!(
                d > 0.004,
                "{a:?} and {b:?} draw the same picture (difference {d:.4}) — \
                 a family that looks like another one tells the player nothing"
            );
        }
    }
}
