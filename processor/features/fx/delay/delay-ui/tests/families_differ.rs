//! Two machines must not draw the same picture. See the reverb's note.

use delay_ui::viz::{DelayUniforms, DelayView, Family, SHADER, Tap};

const W: u32 = 480;
const H: u32 = 140;

fn view(family: Family) -> DelayView {
    let beat = 0.42;
    DelayView {
        taps: (1..=6)
            .map(|i| Tap {
                at: beat * i as f32,
                level: 0.82_f32.powi(i - 1),
                pan: if i % 2 == 1 { -0.85 } else { 0.85 },
            })
            .collect(),
        window: beat * 7.0,
        mix: 1.0,
        on: true,
        beat,
        division: "1/4".into(),
        family,
        color: [59, 130, 246],
        time: 1.9,
    }
}

#[test]
fn every_family_draws_something_and_no_two_draw_the_same_thing() {
    let Some(probe) = fts_audio_ui::shader::probe::Probe::open() else {
        eprintln!("no GPU here — skipping");
        return;
    };

    let families = [
        Family::Digital,
        Family::Tape,
        Family::Analog,
        Family::Pitch,
        Family::Rhythmic,
        Family::Special,
    ];
    let frames: Vec<_> = families
        .iter()
        .map(|&f| {
            let u = DelayUniforms::of(&view(f), W as f32, H as f32);
            (f, probe.render(SHADER, bytemuck::bytes_of(&u), W, H))
        })
        .collect();

    for (family, frame) in &frames {
        assert!(
            frame.coverage() > 0.01,
            "{family:?} drew nothing — a branch that renders an empty panel is \
             indistinguishable from a shader that failed"
        );
    }

    for (i, (a, fa)) in frames.iter().enumerate() {
        for (b, fb) in &frames[i + 1..] {
            let d = fa.difference(fb);
            assert!(
                d > 0.002,
                "{a:?} and {b:?} draw the same picture (difference {d:.4}) — \
                 a family that looks like another one tells the player nothing"
            );
        }
    }
}
