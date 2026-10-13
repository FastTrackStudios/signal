//! Every delay family, rendered by the real shader, to PNGs.
//!
//!     cargo run -p delay-ui --features viz,probe --example family_sheet -- out/
//!
//! The live window is the only other place these can be seen, and getting one
//! there means running the rig and driving the algorithm picker by hand. A
//! family that is awkward to reach is exactly the one that ships unlooked-at,
//! so this reaches all six without a window.

use delay_ui::viz::{DelayUniforms, DelayView, Family, Tap};

const W: u32 = 760;
const H: u32 = 190;

/// A tail of `n` repeats a beat apart, alternating sides — the shape every
/// family is shown making, so the only thing that differs between the frames
/// is the machine.
fn taps(n: usize, beat: f32) -> Vec<Tap> {
    (1..=n)
        .map(|i| Tap {
            at: beat * i as f32,
            level: 0.82_f32.powi(i as i32 - 1),
            pan: if i % 2 == 1 { -0.85 } else { 0.85 },
        })
        .collect()
}

fn view(family: Family, time: f32) -> DelayView {
    let beat = 0.42;
    let taps = taps(6, beat);
    DelayView {
        window: beat * 7.0,
        taps,
        mix: 1.0,
        on: true,
        beat,
        division: "1/4".into(),
        family,
        // `DELAY_COLORS[0]` in the rig: deep blue, not sky.
        color: [59, 130, 246],
        time,
    }
}

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    std::fs::create_dir_all(&dir).expect("output directory");

    let Some(probe) = fts_audio_ui::shader::probe::Probe::open() else {
        eprintln!("no GPU available — nothing to render");
        std::process::exit(2);
    };
    let source = delay_ui::viz::SHADER;

    for family in [
        Family::Digital,
        Family::Tape,
        Family::Analog,
        Family::Pitch,
        Family::Rhythmic,
        Family::Special,
    ] {
        // Two moments, because half of what separates these machines only
        // exists in motion: a tape's wow has moved the repeats by the second
        // frame and a digital delay's have not budged.
        for (tag, time) in [("a", 0.35_f32), ("b", 1.9_f32)] {
            let v = view(family, time);
            let u = DelayUniforms::of(&v, W as f32, H as f32);
            let frame = probe.render(source, bytemuck::bytes_of(&u), W, H);
            let name = format!("{dir}/delay-{}-{tag}.png", label(family));
            write_png(&name, &frame.rgba, W, H);
            println!("{name}  coverage {:.3}", frame.coverage());
        }
    }
}

fn label(f: Family) -> &'static str {
    match f {
        Family::Digital => "digital",
        Family::Tape => "tape",
        Family::Analog => "analog",
        Family::Pitch => "pitch",
        Family::Rhythmic => "rhythmic",
        Family::Special => "special",
    }
}

/// The frames are premultiplied, which a PNG viewer is not expecting; undo it
/// and lay them on the panel's own ground so the sheet looks like the rack.
fn write_png(path: &str, rgba: &[u8], w: u32, h: u32) {
    let mut out = Vec::with_capacity(rgba.len());
    for px in rgba.chunks_exact(4) {
        let a = f32::from(px[3]) / 255.0;
        let over = |c: u8| -> u8 {
            let lit = if a > 0.001 { f32::from(c) / a } else { 0.0 };
            (lit.mul_add(a, 8.0 * (1.0 - a))).clamp(0.0, 255.0) as u8
        };
        out.extend_from_slice(&[over(px[0]), over(px[1]), over(px[2]), 255]);
    }
    let file = std::fs::File::create(path).expect("create png");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .expect("png header")
        .write_image_data(&out)
        .expect("png data");
}
